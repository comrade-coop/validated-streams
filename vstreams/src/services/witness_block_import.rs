//! Block import which waits for all events to be witnessed before finalizing a block.

use crate::{
    configs::FullClient,
    {proofs::EventProofs, services::events::EventService},
};
use futures::{FutureExt, future::Shared};
use node_runtime::{self, opaque::Block, pallet_validated_streams::ExtrinsicDetails};
use sc_consensus::{BlockCheckParams, BlockImportParams, ImportResult};
pub use sc_executor::NativeElseWasmExecutor;
use sp_api::ProvideRuntimeApi;
use sp_consensus::Error as ConsensusError;
use sp_consensus::SyncOracle;
use tokio::sync::oneshot;
use std::sync::Arc;
use sc_network_sync::SyncingService;
/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
#[derive(Clone)]
pub struct WitnessBlockImport<I> {
    parent_block_import: I,
    #[cfg(not(feature = "on-chain-proofs"))]
    client: Arc<FullClient>,
    #[cfg(not(feature = "on-chain-proofs"))]
    event_proofs: Arc<dyn EventProofs + Send + Sync>,
    #[cfg(not(feature = "on-chain-proofs"))]
    sync_service: Shared<oneshot::Receiver<Arc<SyncingService<Block>>>>,
}
impl<I> WitnessBlockImport<I> {
    #[cfg(feature = "on-chain-proofs")]
    pub fn new(parent_block_import: I) -> Self {
        Self { parent_block_import }
    }
    #[cfg(not(feature = "on-chain-proofs"))]
    /// Create a new [WitnessBlockImport]
    pub fn new(
        parent_block_import: I,
        client: Arc<FullClient>,
        event_proofs: Arc<dyn EventProofs + Send + Sync>,
        ) -> (Self, oneshot::Sender<Arc<SyncingService<Block>>>) {
        let (sync_service_sender, sync_service) = oneshot::channel();
        (Self { parent_block_import, client, event_proofs, sync_service: sync_service.shared()}, sync_service_sender)
    }
}
#[async_trait::async_trait]
impl<I: sc_consensus::BlockImport<Block>> sc_consensus::BlockImport<Block> for WitnessBlockImport<I>
where
I: Send,
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
        block: BlockCheckParams<Block>,
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
        let sync_service = self.sync_service.clone().await.unwrap();
            if sync_service.is_major_syncing(){
                log::info!("🔁 Node is Syncing");
                return self
                    .parent_block_import
                    .import_block(block)
                    .await
                    .map_err(|e| ConsensusError::ClientImport(format!("{}", e)))

            }
        if let Some(block_extrinsics) = &block.body {
            // get an iterator for all ready transactions and skip the first element which
            // contains the default extrinsic
            let block_id = self.client.chain_info().best_hash;
            let extrinsic_ids = self
                .client
                .runtime_api()
                .get_extrinsic_ids(block_id, block_extrinsics)
                .ok()
                .unwrap_or_default();
            match EventService::verify_events_validity(
                self.client.clone(),
                self.event_proofs.clone(),
                extrinsic_ids.clone(),
                ) {
                Ok(unwitnessed_ids) =>
                    if !unwitnessed_ids.is_empty() {
                        log::info!("Block rejeceted containing {} unwitnessed events",unwitnessed_ids.len());
                        return Err(ConsensusError::ClientImport(format!("Block contains unwitnessed events")));
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