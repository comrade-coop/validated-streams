use std::{
	collections::HashMap,
	io::{Error, ErrorKind},
	sync::{Arc, Mutex},
};

use crate::streams_server::validated_streams::WitnessedEventRequest;

pub trait EventProofs {
	fn contains(&self, event_id: String) -> Result<bool, Error>;
	fn add_event_proof(&self, event: WitnessedEventRequest, origin: String) -> Result<u16, Error>;
	fn get_proof_count(&self, event_id: String) -> Result<u16, Error>;
}

pub struct InMemoryEventProofs {
	proofs: Arc<Mutex<HashMap<String, HashMap<String, WitnessedEventRequest>>>>,
}
impl InMemoryEventProofs {
	pub fn new() -> Arc<dyn EventProofs + Send + Sync> {
		Arc::new(InMemoryEventProofs { proofs: Arc::new(Mutex::new(HashMap::new())) })
	}
}
impl EventProofs for InMemoryEventProofs {
	// get the event_id from proofs if it does not exist create it and check if origin already sent
	// the proof
	fn add_event_proof(
		&self,
		witnessed_event: WitnessedEventRequest,
		origin: String,
	) -> Result<u16, Error> {
		let event_ref = witnessed_event
			.event
			.as_ref()
			.ok_or(Error::new(ErrorKind::InvalidData, "Could not retreive Stream info"))?;
		let event_id = event_ref.event_id.clone();
		let mut proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		if proofs.entry(event_id.clone()).or_insert(HashMap::new()).contains_key(&origin) {
			log::info!("{} already sent a proof for stream {}", origin, event_id);
			Err(Error::new(ErrorKind::AlreadyExists, "Already sent a proof"))
		} else {
			let proof_count = proofs
				.get(&event_id)
				.ok_or(Error::new(ErrorKind::InvalidData, "failed retreiving proof count"))?
				.len() as u16;
			proofs
				.entry(event_id.clone())
				.or_insert(HashMap::new())
				.insert(origin, witnessed_event);
			Ok(proof_count + 1)
		}
	}
	fn contains(&self, event_id: String) -> Result<bool, Error> {
		let proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		Ok(proofs.contains_key(&event_id))
	}
	fn get_proof_count(&self, event_id: String) -> Result<u16, Error> {
		let proofs = self
			.proofs
			.lock()
			.or(Err(Error::new(ErrorKind::InvalidData, "failed locking InMemoryProofs")))?;
		Ok(proofs
			.get(&event_id)
			.ok_or(Error::new(ErrorKind::InvalidData, "failed retreiving proof Count"))?
			.len() as u16)
	}
}
