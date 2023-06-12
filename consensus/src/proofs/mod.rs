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

#[cfg(feature = "rocksdb")]
pub mod rocksdb;
#[cfg(feature = "rocksdb")]
pub use self::rocksdb::RocksDbEventProofs;

/// Proof of event that has been witnessed; an event id and a signature
/// Signatures do not have a defined cryptosystem, but are assumed to be sr25519 signatures by
/// [super::services::events].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WitnessedEvent {
	/// The signature of the event
	pub signature: Vec<u8>,
	/// The public key which was used to produce the signature
	pub pub_key: CryptoTypePublicPair,
	/// The id/hash of the event
	pub event_id: H256,
}

/// Storage for event proofs (for [WitnessedEvent]-s)
pub trait EventProofsTrait {
	/// Stores the provided event proof.
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<(), Error>;

	/// Returns a [HashMap] containing the public keys and their corresponding signatures for the
	/// given event id and list of validators
	fn get_event_proofs(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error>;

	/// Retrieve count of proof for the given event id. Equivalent to
	/// `self.get_event_proofs(event_id, validators)?.len()`, but possibly more optimal.
	fn get_event_proof_count(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<u16, Error> {
		Ok(self.get_event_proofs(event_id, validators)?.len() as u16)
	}

	/// Remove proofs of the given event observed by validators not in the list of validators passed
	/// in. Useful for maintaining the pool of event proofs whenever the validator set changes.
	fn purge_event_stale_signatures(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<(), Error>;
}
