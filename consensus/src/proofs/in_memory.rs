//! Validated streams event proof types and storage

use super::{EventProofsTrait, WitnessedEvent};
use crate::errors::Error;

use sp_core::H256;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::{
	collections::{hash_map::Entry, HashMap},
	sync::Mutex,
};

/// An in-memory store of event proofs.
pub struct InMemoryEventProofs {
	proofs: Mutex<HashMap<H256, HashMap<CryptoTypePublicPair, Vec<u8>>>>,
}
impl InMemoryEventProofs {
	/// Create an empty [InMemoryEventProofs] instances.
	pub fn new() -> InMemoryEventProofs {
		InMemoryEventProofs { proofs: Mutex::new(HashMap::new()) }
	}
}
impl Default for InMemoryEventProofs {
	fn default() -> Self {
		Self::new()
	}
}
impl EventProofsTrait for InMemoryEventProofs {
	fn add_event_proof(&self, witnessed_event: &WitnessedEvent) -> Result<(), Error> {
		let event_id = witnessed_event.event_id;
		let mut proofs =
			self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;

		let event_witnesses = proofs.entry(event_id).or_default();
		match event_witnesses.entry(witnessed_event.pub_key.clone()) {
			Entry::Vacant(e) => {
				e.insert(witnessed_event.signature.clone());
				Ok(())
			},
			witness_entry => {
				log::info!(
					"{:?} already sent a proof for event {:?}",
					witness_entry.key(),
					event_id
				);
				Ok(())
			},
		}
	}

	fn get_event_proofs(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error> {
		let proofs = self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		Ok(proofs
			.get(event_id)
			.map(|event_proofs| {
				let mut event_proofs = event_proofs.clone();
				event_proofs.retain(|k, _| validators.contains(k));
				event_proofs
			})
			.unwrap_or_default())
	}

	fn purge_event_stale_signatures(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<(), Error> {
		let mut proofs =
			self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		if let Some(event_proofs) = proofs.get_mut(event_id) {
			event_proofs.retain(|k, _| validators.contains(k));
		}
		Ok(())
	}
}
