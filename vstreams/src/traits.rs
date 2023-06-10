//! Traits used by validated streams

use crate::errors::Error;
use async_trait::async_trait;
use sp_core::H256;

/// A trait wrapping the [EventService]'s functionality of witnessing an event, called by the
/// trusted client (e.g. through GRPC).
#[async_trait]
pub trait EventWitnesserTrait {
	/// receives client requests for handling incoming witnessed events
	async fn witness_event(&self, event: H256) -> Result<(), Error>;
}

/// A trait responsible for getting the stream of validated events
#[async_trait]
pub trait EventValidatorTrait {
	/// Get the list of events in a specific block.
	async fn get_finalized_block_events(&self, block_num: u32) -> Result<Vec<H256>, Error>;

	/// Get the latest block.
	async fn get_latest_finalized_block(&self) -> Result<u32, Error>;
}
