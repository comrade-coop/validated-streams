use std::{
	collections::HashMap,
	io::{Error, ErrorKind},
	sync::{Arc, Mutex},
};

use sp_core::H256;

use crate::gossip::WitnessedEvent;

pub trait EventProofs {
	fn contains(&self, event_id: H256) -> Result<bool, Error>;
	fn add_event_proof(&self, event: &WitnessedEvent, origin: Vec<u8>) -> Result<u16, Error>;
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error>;
	fn verify_event_validity(&self, event_id: H256) -> Result<bool, Error>;
	fn verify_events_validity(&self, ids: Vec<H256>) -> Result<Vec<H256>, Error>;
	fn set_target(&self, target: u16) -> Result<bool, Error>;
}

pub struct InMemoryEventProofs {
	target: Mutex<u16>,
	//map event ids to provided senders of event proofs
	proofs: Arc<Mutex<HashMap<H256, HashMap<Vec<u8>, WitnessedEvent>>>>,
}
impl InMemoryEventProofs {
	pub fn new() -> Arc<dyn EventProofs + Send + Sync> {
		Arc::new(InMemoryEventProofs {
			proofs: Arc::new(Mutex::new(HashMap::new())),
			target: Mutex::new(0),
		})
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
		let event_id = witnessed_event.event_id.clone();
		let mut proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		if proofs.entry(event_id.clone()).or_insert(HashMap::new()).contains_key(&origin) {
			log::info!("{:?} already sent a proof for event {:?}", origin, event_id);
			Err(Error::new(ErrorKind::AlreadyExists, "Already sent a proof"))
		} else {
			let proof_count = proofs
				.get(&event_id)
				.ok_or(Error::new(ErrorKind::InvalidData, "event was not inserted"))?
				.len() as u16;
			proofs
				.entry(event_id.clone())
				.or_insert(HashMap::new())
				.insert(origin, witnessed_event.clone());
			Ok(proof_count + 1)
		}
	}
	fn contains(&self, event_id: H256) -> Result<bool, Error> {
		let proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		Ok(proofs.contains_key(&event_id))
	}
	fn get_proof_count(&self, event_id: H256) -> Result<u16, Error> {
		let proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		if proofs.contains_key(&event_id) {
			let count = proofs
				.get(&event_id)
				.ok_or(Error::new(ErrorKind::InvalidData, "Could not retreive event count"))?
				.len() as u16;
			Ok(count)
		} else {
			Ok(0)
		}
	}
	fn verify_event_validity(&self, event_id: H256) -> Result<bool, Error> {
		if self.contains(event_id.clone())? {
			let current_count = self.get_proof_count(event_id)?;
			if current_count <
				*self
					.target
					.lock()
					.or(Err(Error::new(ErrorKind::InvalidData, "failed locking target")))?
			{
				Ok(true)
			} else {
				Ok(false)
			}
		} else {
			Ok(false)
		}
	}
	fn verify_events_validity(&self, ids: Vec<H256>) -> Result<Vec<H256>, Error> {
		let mut unprepared_ids = Vec::new();
		let target = *self
			.target
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking target")))?;
		for id in ids {
			if self.contains(id.clone())? {
				let current_count = self.get_proof_count(id.clone())?;
				if current_count < target {
					unprepared_ids.push(id);
				}
			} else {
				unprepared_ids.push(id);
			}
		}
		Ok(unprepared_ids)
	}
	fn set_target(&self, val: u16) -> Result<bool, Error> {
		*self
			.target
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking target")))? = val;
		Ok(true)
	}
}
