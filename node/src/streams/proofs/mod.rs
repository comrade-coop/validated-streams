//! Validated streams event proof types and storage

use crate::streams::errors::Error;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use std::{
	collections::{hash_map::Entry, HashMap},
	sync::Mutex,
};

#[cfg(test)]
pub mod tests;

/// Represents an event that has been witnessed along with its signature
/// Signatures do not have a defined cryptosystem, but are assumed to be sr25519 signatures by
/// [super::services::events].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WitnessedEvent {
	/// The signature of the event
	pub signature: Vec<u8>,
	/// The public key used to produce the signature
	pub pub_key: Vec<u8>,
	/// The id/hash of the event being witnessed
	pub event_id: H256,
}

/// Storage for Event proofs
pub trait EventProofs {
	/// adds an event proof from the given witnessed event if it has not yet been added
	fn add_event_proof(&self, event: &WitnessedEvent, origin: Vec<u8>) -> Result<u16, Error>;
	/// retrieve the proof count for the given event id
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error>;
}

type ProofsMap = HashMap<H256, HashMap<Vec<u8>, WitnessedEvent>>;

/// An in-memory store of event proofs.
pub struct InMemoryEventProofs {
	// maps event ids to provided senders of event proofs
	proofs: Mutex<ProofsMap>,
}
impl InMemoryEventProofs {
	/// Create a new [InMemoryEventProofs] instance
	pub fn create() -> InMemoryEventProofs {
		InMemoryEventProofs { proofs: Mutex::new(HashMap::new()) }
	}
}
impl EventProofs for InMemoryEventProofs {
	// get the event_id from proofs if it does not exist create it and check if origin already sent
	// the proof
	fn add_event_proof(
		&self,
		witnessed_event: &WitnessedEvent,
		origin: Vec<u8>,
	) -> Result<u16, Error> {
		let event_id = witnessed_event.event_id;
		let mut proofs =
			self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;

		let event_witnesses = proofs.entry(event_id).or_default();
		let event_witnesses_count = event_witnesses.len() as u16;
		match event_witnesses.entry(origin) {
			Entry::Vacant(e) => {
				e.insert(witnessed_event.clone());
				Ok(event_witnesses_count + 1)
			},
			witness_entry => {
				log::info!(
					"{:?} already sent a proof for event {:?}",
					witness_entry.key(),
					event_id
				);
				Err(Error::AlreadySentProof(event_id))
			},
		}
	}
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error> {
		let proofs = self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		if proofs.contains_key(&event_id) {
			let count = proofs
				.get(&event_id)
				.ok_or_else(|| Error::Other("Could not retrieve event count".to_string()))?
				.len() as u16;
			Ok(count)
		} else {
			Ok(0)
		}
	}
}
