//! Block import which waits for all events to be witnessed before finalizing a block.
#![cfg(feature = "off-chain-proofs")]

use crate::{
	events::{verify_events_validity, AuthoritiesList},
	proofs::EventProofsTrait,
};
use codec::Codec;
use futures::{future::Shared, FutureExt};
use lru::LruCache;
use pallet_validated_streams::ValidatedStreamsApi;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};

use sp_api::{HeaderT, ProvideRuntimeApi};
use sp_consensus::{Error as ConsensusError, SyncOracle};
use sp_consensus_aura::AuraApi;
use sp_runtime::{app_crypto::CryptoTypePublicPair, traits::Block as BlockT};
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

/// Wrapper around a [BlockImport] which expects all events in the block to be witnessed in an
/// [EventProofs] instance before allowing the block to pass to the the next import -- thus
/// precluding nodes from building on top of blocks containing unwitnessed events and preventing the
/// the finalization of blocks that lack sufficient signatures. Currently, when the node falls
/// behind (as determined by [SyncingService::is_major_syncing]), the node will start forwarding
/// blocks directly to the internal [BlockImport], regardless of signatures. This is intended to
/// help in cases where a single missing event proof hangs the whole node.
pub struct ValidatedStreamsBlockImport<
	Block: BlockT,
	I,
	Client,
	EventProofs,
	SyncingService,
	AuthorityId,
> {
	parent_block_import: I,
	client: Arc<Client>,
	event_proofs: Arc<EventProofs>,
	sync_service: Shared<oneshot::Receiver<Arc<SyncingService>>>,
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash, AuthoritiesList>>>,
	phantom: std::marker::PhantomData<(Block, AuthorityId)>,
}

impl<Block: BlockT, I, Client, EventProofs, SyncingService, AuthorityId>
	ValidatedStreamsBlockImport<Block, I, Client, EventProofs, SyncingService, AuthorityId>
{
	/// Create a new [ValidatedStreamsBlockImport]
	pub fn new(
		parent_block_import: I,
		client: Arc<Client>,
		event_proofs: Arc<EventProofs>,
		block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash, AuthoritiesList>>>,
	) -> (Self, impl FnOnce(Arc<SyncingService>)) {
		let (sync_service_sender, sync_service_receiver) = oneshot::channel();

		(
			Self {
				parent_block_import,
				client,
				event_proofs,
				sync_service: sync_service_receiver.shared(),
				block_state,
				phantom: PhantomData,
			},
			move |sync_service| {
				let _ = sync_service_sender.send(sync_service);
			},
		)
	}
}

impl<Block: BlockT, I: Clone, Client, EventProofs, SyncingService, AuthorityId> Clone
	for ValidatedStreamsBlockImport<Block, I, Client, EventProofs, SyncingService, AuthorityId>
{
	fn clone(&self) -> Self {
		Self {
			parent_block_import: self.parent_block_import.clone(),
			client: self.client.clone(),
			event_proofs: self.event_proofs.clone(),
			sync_service: self.sync_service.clone(),
			block_state: self.block_state.clone(),
			phantom: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<
		Block: BlockT,
		I: BlockImport<Block, Error = ConsensusError> + Send + Sync,
		EventProofs: EventProofsTrait + Send + Sync,
		Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
		SyncingService: SyncOracle + Send + Sync,
		AuthorityId: Codec + Send + Sync + 'static,
	> BlockImport<Block>
	for ValidatedStreamsBlockImport<Block, I, Client, EventProofs, SyncingService, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
{
	type Error = ConsensusError;
	type Transaction = I::Transaction;

	async fn check_block(
		&mut self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		return self.parent_block_import.check_block(block).await
	}

	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		let sync_service = self.sync_service.clone().await.unwrap();
		if sync_service.is_major_syncing() {
			log::info!("🔁 Node is Syncing");
			return self.parent_block_import.import_block(block).await
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
				self.block_state.clone(),
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
					} else if !extrinsic_ids.is_empty() {
						log::info!(
							"👌 block {} contains {} events, All have been witnessed",
							block.post_hash(),
							extrinsic_ids.len()
						);
					},
				Err(e) => {
					log::error!("the following Error happened while verifying block events in the event_proofs:{}",e);
					return Err(ConsensusError::ClientImport(e.to_string()))
				},
			}
		}

		return self.parent_block_import.import_block(block).await
	}
}
