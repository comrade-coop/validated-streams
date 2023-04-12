//! Validated streams event proof types and storage

use crate::streams::errors::Error;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_runtime::app_crypto::CryptoTypePublicPair;
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
	pub pub_key: CryptoTypePublicPair,
	/// The id/hash of the event being witnessed
	pub event_id: H256,
}

/// Storage for Event proofs
pub trait EventProofs {
	/// adds an event proof from the given witnessed event if it has not yet been added
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<u16, Error>;
	/// adds all the event proofs
	fn add_events_proofs(&self, proofs: ProofsMap) -> Result<(), Error>;
	/// retrieve the proof count for the given event id
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error>;
	/// returns a `HashMap` containing the public keys and their corresponding signatures for the
	/// given event id
	fn get_event_proofs(
		&self,
		event_id: &H256,
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error>;
	/// returns [ProofsMap] for the given events
	fn get_events_proofs(&self, events: &[H256]) -> Result<ProofsMap, Error>;
	/// remove stale signatures from events observed by previous validators based on the
	/// updated list of validators.
	fn purge_stale_signatures(
		&self,
		validators: &[CryptoTypePublicPair],
		events: &[H256],
	) -> Result<(), Error>;
	/// remove stale signatures of the given event observed by previous validators based on the
	/// updated list of validators and return the updated proof count
	fn purge_stale_signature(
		&self,
		validators: &[CryptoTypePublicPair],
		event_id: H256,
	) -> Result<u16, Error>;
}
/// map event ids to their proofs
pub type ProofsMap = HashMap<H256, HashMap<CryptoTypePublicPair, Vec<u8>>>;

/// An in-memory store of event proofs.
pub struct InMemoryEventProofs {
	// maps event ids to provided senders of event proofs
	proofs: Mutex<ProofsMap>,
}
impl InMemoryEventProofs {
	/// Create a new [InMemoryEventProofs] instance
	#[allow(dead_code)]
	pub fn create() -> InMemoryEventProofs {
		InMemoryEventProofs { proofs: Mutex::new(HashMap::new()) }
	}
}
impl EventProofs for InMemoryEventProofs {
	// get the event_id from proofs if it does not exist create it and check if origin already sent
	// the proof
	fn add_event_proof(&self, witnessed_event: &WitnessedEvent) -> Result<u16, Error> {
		let event_id = witnessed_event.event_id;
		let mut proofs =
			self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;

		let event_witnesses = proofs.entry(event_id).or_default();
		let event_witnesses_count = event_witnesses.len() as u16;
		match event_witnesses.entry(witnessed_event.pub_key.clone()) {
			Entry::Vacant(e) => {
				e.insert(witnessed_event.signature.clone());
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
	fn add_events_proofs(&self, _proofs: ProofsMap) -> Result<(), Error> {
		todo!()
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
	fn get_event_proofs(
		&self,
		event_id: &H256,
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error> {
		let proofs = self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		if proofs.contains_key(&event_id) {
			let map = proofs
				.get(event_id)
				.ok_or_else(|| Error::Other("Could not retrieve event proofs".to_string()))?
				.clone();
			Ok(map)
		} else {
			Err(Error::Other("Event not found".to_string()))
		}
	}
	fn get_events_proofs(&self, _events: &[H256]) -> Result<ProofsMap, Error> {
		Ok(ProofsMap::new())
	}

	fn purge_stale_signatures(
		&self,
		validators: &[CryptoTypePublicPair],
		events: &[H256],
	) -> Result<(), Error> {
		for event in events.iter() {
			let mut proofs =
				self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
			if proofs.contains_key(event) {
				proofs
					.get_mut(event)
					.ok_or_else(|| {
						Error::Other("Could not retrieve event from event proofs".to_string())
					})?
					.retain(|k, _| validators.contains(k));
			}
		}
		Ok(())
	}

	fn purge_stale_signature(
		&self,
		validators: &[CryptoTypePublicPair],
		event_id: H256,
	) -> Result<u16, Error> {
		let mut proofs =
			self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		if proofs.contains_key(&event_id) {
			proofs
				.get_mut(&event_id)
				.ok_or_else(|| {
					Error::Other("Could not retrieve event from event proofs".to_string())
				})?
				.retain(|k, _| validators.contains(k));
		}
		Ok(proofs
			.get(&event_id)
			.ok_or_else(|| Error::Other("Could not retrieve event from event proofs".to_string()))?
			.len() as u16)
	}
}
/// persistent database for storing event proofs
pub struct ProofStore {
	db: sled::Db,
}
impl ProofStore {
	/// returns a ProofStore instance that persists data in the provided path
	pub fn create(path: &str) -> Self {
		Self { db: sled::open(path).expect("open") }
	}
	/// inserts the given event proofs and check whether they already exist in the database
	fn insert_proofs(
		&self,
		event_id: &H256,
		proofs: HashMap<CryptoTypePublicPair, Vec<u8>>,
	) -> Result<u16, Error> {
		let mut witnesses: HashMap<CryptoTypePublicPair, Vec<u8>> =
			if let Some(existing_witnesses) = self.get_proofs(&event_id) {
				for (key, _) in &proofs {
					if existing_witnesses.contains_key(key) {
						return Err(Error::AlreadySentProof(event_id.clone()))
					}
				}
				existing_witnesses
			} else {
				HashMap::new()
			};
		for proof in &proofs {
			witnesses.insert(proof.0.clone(), proof.1.clone());
		}
		self.update_proofs(event_id, &witnesses)?;
		Ok(witnesses.len() as u16)
	}
	/// overwrite the event proofs with new ones
	fn update_proofs(
		&self,
		event_id: &H256,
		proofs: &HashMap<CryptoTypePublicPair, Vec<u8>>,
	) -> Result<(), Error> {
		let serialized_witnesses =
			bincode::serialize(&proofs).map_err(|e| Error::Other(e.to_string()))?;
		self.db
			.insert(event_id, serialized_witnesses)
			.map_err(|e| Error::Other(e.to_string()))?;
		Ok(())
	}
	/// retreives the proofs of the event id
	fn get_proofs(&self, event_id: &H256) -> Option<HashMap<CryptoTypePublicPair, Vec<u8>>> {
		self.db.get(event_id).ok()?.map(|value| bincode::deserialize(&value).unwrap())
	}
}
impl EventProofs for ProofStore {
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<u16, Error> {
		self.insert_proofs(
			&event.event_id,
			HashMap::from([(event.pub_key.clone(), event.signature.clone())]),
		)
	}

	fn add_events_proofs(&self, proofs: ProofsMap) -> Result<(), Error> {
		for (event, proof) in proofs {
			self.insert_proofs(&event, proof)?;
		}
		Ok(())
	}

	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error> {
		if let Some(proofs) = self.get_proofs(&event_id) {
			Ok(proofs.len() as u16)
		} else {
			Ok(0)
		}
	}
	fn get_event_proofs(
		&self,
		event_id: &H256,
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error> {
		if let Some(proofs) = self.get_proofs(event_id) {
			Ok(proofs)
		} else {
			Err(Error::Other("Event not found".to_string()))
		}
	}

	fn get_events_proofs(&self, events: &[H256]) -> Result<ProofsMap, Error> {
		let mut proofs_map = ProofsMap::new();
		for event in events {
			if let Some(proofs) = self.get_proofs(event) {
				proofs_map.insert(event.clone(), proofs);
			} else {
				return Err(Error::Other("Event not found".to_string()))
			}
		}
		Ok(proofs_map)
	}

	fn purge_stale_signatures(
		&self,
		validators: &[CryptoTypePublicPair],
		events: &[H256],
	) -> Result<(), Error> {
		for event_id in events {
			if let Some(mut proofs) = self.get_proofs(event_id) {
				proofs.retain(|k, _| validators.contains(k));
				self.update_proofs(event_id, &proofs)?;
			}
		}
		Ok(())
	}

	fn purge_stale_signature(
		&self,
		validators: &[CryptoTypePublicPair],
		event_id: H256,
	) -> Result<u16, Error> {
		if let Some(mut proofs) = self.get_proofs(&event_id) {
			proofs.retain(|k, _| validators.contains(k));
			self.update_proofs(&event_id, &proofs)?;
			Ok(proofs.len() as u16)
		} else {
			return Err(Error::Other("Event not found".to_string()))
		}
	}
}
