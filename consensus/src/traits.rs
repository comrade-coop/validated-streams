//! Traits used by Validated Streams code

use crate::errors::Error;
use async_trait::async_trait;
use sp_core::H256;

/// A trait wrapping the functionality of witnessing an event that is called by the trusted client
/// (e.g. through GRPC).
#[async_trait]
pub trait EventWitnesserTrait {
	/// Witnesses an event by signing it with the key of the current node and gossipping the
	/// signature to all peers.
	async fn witness_event(&self, event: H256) -> Result<(), Error>;
}

/// A trait responsible for getting a stream of validated/finalized events from the node to a
/// client. Note that there is nothing this trait does which can't be done through the RPC / a light
/// client.
#[async_trait]
pub trait EventValidatorTrait {
	/// Get the list of events in a specific block. If the block is not ready yet, waits until the
	/// block is finalized. To use as a stream of events, just query the events in successive block
	/// numbers.
	async fn get_finalized_block_events(&self, block_num: u32) -> Result<Vec<H256>, Error>;

	/// Get the latest block's number.
	async fn get_latest_finalized_block(&self) -> Result<u32, Error>;
}
