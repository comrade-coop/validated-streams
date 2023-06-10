//! Service which returns the stream of validated events

use crate::{errors::Error, traits::EventValidatorTrait};
use async_trait::async_trait;
use futures::StreamExt;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockBackend, BlockchainEvents, HeaderBackend};
use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_blockchain::{lowest_common_ancestor, HeaderMetadata};
use sp_core::H256;
use std::{marker::PhantomData, sync::Arc};

/// Service which returns the stream of validated events.
pub struct EventValidator<Client, Block> {
	client: Arc<Client>,
	phantom: PhantomData<Block>,
}

impl<Client, Block> EventValidator<Client, Block> {
	/// Create a new EventValidator for a specific client.
	/// Note that you might need to call this like `EventValidator::<Client, Block>::new(..)`
	/// because of generics
	pub fn new(client: Arc<Client>) -> Self {
		Self { client, phantom: PhantomData }
	}
}

#[async_trait]
impl<Client, Block: BlockT> EventValidatorTrait for EventValidator<Client, Block>
where
	Client: HeaderBackend<Block>
		+ HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>
		+ Sync
		+ Send
		+ 'static,
	Client::Api: ExtrinsicDetails<Block>,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
{
	async fn get_finalized_block_events(&self, block_num: u32) -> Result<Vec<H256>, Error> {
		let mut last_finalized = self.client.info().finalized_hash;

		let block_id = loop {
			if let Ok(Some(block_hash)) = self.client.block_hash(block_num.into()) {
				// If the block at block_num is part of the chain...
				if let Ok(common_ancestor) =
					lowest_common_ancestor(self.client.as_ref(), last_finalized, block_hash)
				{
					if common_ancestor.hash == block_hash {
						// ...And is part of the finalized chain (LCA between it and the
						// finalized tip is the block itself)
						break block_hash // Then, the block at block_num id was finalized
					}
				}
			}
			// Otherwise, wait for the next change of the finalized chain and try again
			last_finalized =
				self.client.finality_notification_stream().select_next_some().await.hash;
		};

		let block_extrinsics = self.client.block_body(block_id).ok().flatten().unwrap_or_default();

		Ok(self
			.client
			.runtime_api()
			.get_extrinsic_ids(block_id, &block_extrinsics)
			.unwrap_or_default())
	}

	async fn get_latest_finalized_block(&self) -> Result<u32, Error> {
		Ok(self.client.info().finalized_number.into())
	}
}
