use std::{
	collections::{hash_map::Entry, HashMap},
	sync::Mutex,
};
#[cfg(test)]
pub mod tests;
use crate::streams::{errors::Error, services::events::WitnessedEvent};
use sp_core::H256;
pub trait EventProofs {
	fn contains(&self, event_id: H256) -> Result<bool, Error>;
	fn add_event_proof(&self, event: &WitnessedEvent, origin: Vec<u8>) -> Result<u16, Error>;
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error>;
}

type ProofsMap = HashMap<H256, HashMap<Vec<u8>, WitnessedEvent>>;

pub struct InMemoryEventProofs {
	//map event ids to provided senders of event proofs
	proofs: Mutex<ProofsMap>,
}
impl InMemoryEventProofs {
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
	fn contains(&self, event_id: H256) -> Result<bool, Error> {
		let proofs = self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		Ok(proofs.contains_key(&event_id))
	}
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error> {
		let proofs = self.proofs.lock().or(Err(Error::LockFail("InMemoryProofs".to_string())))?;
		if proofs.contains_key(&event_id) {
			let count = proofs
				.get(&event_id)
				.ok_or_else(|| Error::Other("Could not retreive event count".to_string()))?
				.len() as u16;
			Ok(count)
		} else {
			Ok(0)
		}
	}
}
