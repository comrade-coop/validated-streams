//! Block import which waits for all events to be witnessed before finalizing a block.
#![cfg(not(feature = "on-chain-proofs"))]

use crate::{proofs::EventProofs, services::events::verify_events_validity};
use codec::Codec;
use futures::{future::Shared, FutureExt};
use pallet_validated_streams::ExtrinsicDetails;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};

use sp_api::{HeaderT, ProvideRuntimeApi};
use sp_consensus::{Error as ConsensusError, SyncOracle};
use sp_consensus_aura::AuraApi;

use sp_runtime::{app_crypto::CryptoTypePublicPair, traits::Block as BlockT};
use std::{marker::PhantomData, sync::Arc};
use tokio::sync::oneshot;

/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
pub struct WitnessBlockImport<Block: BlockT, I, Client, SyncingService, AuthorityId> {
	parent_block_import: I,
	client: Arc<Client>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	sync_service: Shared<oneshot::Receiver<Arc<SyncingService>>>,
	phantom: std::marker::PhantomData<(Block, AuthorityId)>,
}

impl<Block: BlockT, I, Client, SyncingService, AuthorityId>
	WitnessBlockImport<Block, I, Client, SyncingService, AuthorityId>
{
	/// Create a new [WitnessBlockImport]
	pub fn new(
		parent_block_import: I,
		client: Arc<Client>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> (Self, impl FnOnce(Arc<SyncingService>)) {
		let (sync_service_sender, sync_service_receiver) = oneshot::channel();

		(
			Self {
				parent_block_import,
				client,
				event_proofs,
				sync_service: sync_service_receiver.shared(),
				phantom: PhantomData,
			},
			move |sync_service| {
				let _ = sync_service_sender.send(sync_service);
			},
		)
	}
}

impl<Block: BlockT, I: Clone, Client, SyncingService, AuthorityId> Clone
	for WitnessBlockImport<Block, I, Client, SyncingService, AuthorityId>
{
	fn clone(&self) -> Self {
		Self {
			parent_block_import: self.parent_block_import.clone(),
			client: self.client.clone(),
			event_proofs: self.event_proofs.clone(),
			sync_service: self.sync_service.clone(),
			phantom: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<
		Block: BlockT,
		I: BlockImport<Block> + Send + Sync,
		Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
		SyncingService: SyncOracle + Send + Sync,
		AuthorityId: Codec + Send + Sync + 'static,
	> BlockImport<Block> for WitnessBlockImport<Block, I, Client, SyncingService, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	type Error = ConsensusError;
	type Transaction = I::Transaction;

	async fn check_block(
		&mut self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		return self
			.parent_block_import
			.check_block(block)
			.await
			.map_err(|e| ConsensusError::ClientImport(format!("{e}")))
	}

	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		let sync_service = self.sync_service.clone().await.unwrap();
		if sync_service.is_major_syncing() {
			log::info!("🔁 Node is Syncing");
			return self
				.parent_block_import
				.import_block(block)
				.await
				.map_err(|e| ConsensusError::ClientImport(format!("{e}")))
		}

		if let Some(block_extrinsics) = &block.body {
			let parent_block_id = *block.header.parent_hash();
			let extrinsic_ids = self
				.client
				.runtime_api()
				.get_extrinsic_ids(parent_block_id, block_extrinsics)
				.ok()
				.unwrap_or_default();
			match verify_events_validity(
				self.client.clone(),
				parent_block_id,
				self.event_proofs.clone(),
				extrinsic_ids.clone(),
			) {
				Ok(unwitnessed_ids) =>
					if !unwitnessed_ids.is_empty() {
						log::info!(
							"❌ Block rejeceted containing {} unwitnessed events",
							unwitnessed_ids.len()
						);
						return Err(ConsensusError::ClientImport(
							"Block contains unwitnessed events".to_string(),
						))
					} else {
						if extrinsic_ids.len() > 0 {
							log::info!(
								"👌 block {} contains {} events, All have been witnessed",
								block.post_hash(),
								extrinsic_ids.len()
							);
						}
					},
				Err(e) => {
					log::error!("the following Error happened while verifying block events in the event_proofs:{}",e);
				},
			}
		}

		return self
			.parent_block_import
			.import_block(block)
			.await
			.map_err(|e| ConsensusError::ClientImport(format!("{e}")))
	}
}
