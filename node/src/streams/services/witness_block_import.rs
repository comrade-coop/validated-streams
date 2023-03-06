//! Block import which waits for all events to be witnessed before finalizing a block.

use crate::{
	service::FullClient,
	streams::{proofs::EventProofs, services::events::EventService},
};
use log::info;
use node_runtime::{self, opaque::Block, pallet_validated_streams::ExtrinsicDetails};
use sc_consensus::{BlockCheckParams, BlockImportParams, ImportResult};
pub use sc_executor::NativeElseWasmExecutor;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::well_known_cache_keys;
use sp_consensus::Error as ConsensusError;
use sp_runtime::generic::BlockId;
use std::{collections::HashMap, sync::Arc};

/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
#[derive(Clone)]
pub struct WitnessBlockImport<I> {
	parent_block_import: I,
	client: Arc<FullClient>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
}
impl<I> WitnessBlockImport<I> {
	/// Create a new [WitnessBlockImport]
	pub fn new(
		parent_block_import: I,
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> Self {
		Self { parent_block_import, client, event_proofs }
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
		let parent_result = self.parent_block_import.check_block(block).await;
		match parent_result {
			Ok(result) => {
				info!("👌Block Checked");
				return Ok(result)
			},
			Err(e) => return Err(ConsensusError::ClientImport(format!("{}", e))),
		}
	}

	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
		cache: HashMap<well_known_cache_keys::Id, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		if let Some(block_extrinsics) = &block.body {
			// get an iterator for all ready transactions and skip the first element which
			// contains the default extrinsic
			let block_id = BlockId::Number(self.client.chain_info().best_number);
			let extrinsic_ids = self
				.client
				.runtime_api()
				.get_extrinsic_ids(&block_id, block_extrinsics)
				.ok()
				.unwrap_or_default();
			match EventService::verify_events_validity(
				self.client.clone(),
				self.event_proofs.clone(),
				extrinsic_ids.clone(),
			) {
				Ok(unprepared_ids) =>
					if !unprepared_ids.is_empty() {
						log::info!("Block should be deferred as it contains unwitnessed events");
					} else {
						log::info!("All block events have been witnessed:{:?}", extrinsic_ids);
					},
				Err(e) => {
					log::error!("the following Error happened while verifying block events in the event_proofs:{}",e);
				},
			}
		}
		let parent_result = self.parent_block_import.import_block(block, cache).await;
		match parent_result {
			Ok(result) => return Ok(result),
			Err(e) => return Err(ConsensusError::ClientImport(format!("{}", e))),
		}
	}
}
