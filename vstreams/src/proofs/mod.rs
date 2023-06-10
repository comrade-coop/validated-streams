//! Validated streams event proof types and storage

use crate::errors::Error;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::collections::HashMap;

#[cfg(test)]
pub mod tests;

pub mod in_memory;
pub use in_memory::InMemoryEventProofs;

pub mod offchain;
pub use offchain::OffchainStorageEventProofs;

mod rocksdb;
pub use self::rocksdb::RocksDbEventProofs;

/// Represents an event that has been witnessed along with its signature
/// Signatures do not have a defined cryptosystem, but are assumed to be sr25519 signatures by
/// [super::services::events].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WitnessedEvent {
	/// The signature of the event
	pub signature: Vec<u8>,
	/// The public key used to produce the signature
	pub pub_key: CryptoTypePublicPair,
	/// The id/hash of the event being witnessed
	pub event_id: H256,
}

/// Storage for Event proofs
pub trait EventProofsTrait {
	/// adds an event proof to the given witnessed event, creating the event if it does not exist
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<(), Error>;

	/// returns a `HashMap` containing the public keys and their corresponding signatures for the
	/// given event id and validators
	fn get_event_proofs(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error>;
	/// retrieve the proof count for the given event id
	fn get_event_proof_count(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<u16, Error> {
		Ok(self.get_event_proofs(event_id, validators)?.len() as u16)
	}

	/// remove stale signatures of the given event observed by previous validators based on the
	/// updated list of validators and return the updated proof count
	fn purge_event_stale_signatures(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<(), Error>;
}
