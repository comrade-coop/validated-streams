//! Block import which waits for all events to be witnessed before finalizing a block.
#![allow(unused_imports)]
use crate::{
	errors::Error,
	proofs::{EventProofs, ProofsMap},
	services::events::verify_events_validity,
};
use codec::{Codec, Decode, Encode};
use futures::{future::Shared, FutureExt, StreamExt};
use pallet_validated_streams::ExtrinsicDetails;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sc_network::{
	DhtEvent, Event, KademliaKey, NetworkDHTProvider, NetworkEventStream, NetworkService,
};
use sc_network_sync::SyncingService;
use sp_api::{HeaderT, ProvideRuntimeApi};
use sp_consensus::{Error as ConsensusError, SyncOracle};
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::{
	app_crypto::{CryptoTypePublicPair, RuntimePublic},
	generic::BlockId,
	traits::{Block as BlockT, BlockIdTo},
};
use std::{cell::RefCell, marker::PhantomData, sync::Arc};
use tokio::sync::{oneshot, Mutex};

/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
pub struct WitnessBlockImport<Block: BlockT, I, Client, AuthorityId> {
	parent_block_import: I,
	#[cfg(not(feature = "on-chain-proofs"))]
	pub utils: Arc<Utils<Block, Client, AuthorityId>>,
}

impl<Block: BlockT, I, Client, AuthorityId> WitnessBlockImport<Block, I, Client, AuthorityId> {
	#[cfg(feature = "on-chain-proofs")]
	pub fn new(parent_block_import: I) -> Self {
		Self { parent_block_import }
	}
	#[cfg(not(feature = "on-chain-proofs"))]
	/// Create a new [WitnessBlockImport]
	pub fn new(
		parent_block_import: I,
		client: Arc<Client>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> Self {
		Self { parent_block_import, utils: Arc::new(Utils::new(client, event_proofs)) }
	}
}

impl<Block: BlockT, I: Clone, Client, AuthorityId> Clone
	for WitnessBlockImport<Block, I, Client, AuthorityId>
{
	fn clone(&self) -> Self {
		Self {
			parent_block_import: self.parent_block_import.clone(),
			#[cfg(not(feature = "on-chain-proofs"))]
			utils: self.utils.clone(),
		}
	}
}

#[cfg(not(feature = "on-chain-proofs"))]
pub struct Utils<Block: BlockT, Client, AuthorityId> {
	client: Arc<Client>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	sync_service: Shared<oneshot::Receiver<Arc<SyncingService<Block>>>>,
	// #[allow(clippy::type_complexity)]
	sync_service_sender: Arc<Mutex<Option<oneshot::Sender<Arc<SyncingService<Block>>>>>>,
	phantom: std::marker::PhantomData<AuthorityId>,
}

#[cfg(not(feature = "on-chain-proofs"))]
impl<Block: BlockT, Client, AuthorityId> Utils<Block, Client, AuthorityId> {
	pub fn new(client: Arc<Client>, event_proofs: Arc<dyn EventProofs + Send + Sync>) -> Self {
		let (sync_service_sender, sync_service) = oneshot::channel();
		Self {
			client,
			event_proofs,
			sync_service: sync_service.shared(),
			sync_service_sender: Arc::new(Mutex::new(Some(sync_service_sender))),
			phantom: PhantomData,
		}
	}

	pub async fn update_sync_service(self: Arc<Self>, sync_service: Arc<SyncingService<Block>>) {
		if let Some(sync_service_sender) =
			std::mem::replace(&mut *self.sync_service_sender.lock().await, None)
		{
			let _ = sync_service_sender.send(sync_service.clone());
		}
	}
}

impl<Block: BlockT, Client, AuthorityId> Clone for Utils<Block, Client, AuthorityId> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			event_proofs: self.event_proofs.clone(),
			sync_service: self.sync_service.clone(),
			sync_service_sender: self.sync_service_sender.clone(),
			phantom: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<
		Block: BlockT,
		I: BlockImport<Block> + Send + Sync,
		Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
		AuthorityId: Codec + Send + Sync + 'static,
	> BlockImport<Block> for WitnessBlockImport<Block, I, Client, AuthorityId>
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
			.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
	}
	#[cfg(feature = "on-chain-proofs")]
	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		return self
			.parent_block_import
			.import_block(block)
			.await
			.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
	}

	#[cfg(not(feature = "on-chain-proofs"))]
	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		let sync_service = self.utils.sync_service.clone().await.unwrap();
		if sync_service.is_major_syncing() {
			log::info!("🔁 Node is Syncing");
			return self
				.parent_block_import
				.import_block(block)
				.await
				.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
		}

		if let Some(block_extrinsics) = &block.body {
			let parent_block_id = *block.header.parent_hash();
			let extrinsic_ids = self
				.utils
				.client
				.runtime_api()
				.get_extrinsic_ids(parent_block_id, block_extrinsics)
				.ok()
				.unwrap_or_default();
			match verify_events_validity(
				self.utils.client.clone(),
				parent_block_id,
				self.utils.event_proofs.clone(),
				extrinsic_ids.clone(),
			) {
				Ok(unwitnessed_ids) =>
					if !unwitnessed_ids.is_empty() {
						log::info!(
							"Block rejeceted containing {} unwitnessed events",
							unwitnessed_ids.len()
						);
						return Err(ConsensusError::ClientImport(format!(
							"Block contains unwitnessed events"
						)))
					} else {
						log::info!("All block {} events have been witnessed", extrinsic_ids.len());
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
			.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
	}
}
