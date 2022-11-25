use std::{
	collections::HashMap,
	io::{Error, ErrorKind},
	sync::{Arc, Mutex},
};

use crate::streams_server::validated_streams::WitnessedEventRequest;

pub trait EventProofs {
	fn contains(&self, event_id: String) -> bool;
	fn add_event_proof(&self, event: WitnessedEventRequest, origin: String) -> Result<u16, Error>;
	fn get_proof_count(&self, event_id: &str) -> u16;
}

pub struct InMemoryEventProofs {
	proofs: Arc<Mutex<HashMap<String, Vec<WitnessedEventRequest>>>>,
	verification_list: Arc<Mutex<HashMap<String, Vec<String>>>>,
}
impl InMemoryEventProofs {
	pub fn new() -> InMemoryEventProofs {
		InMemoryEventProofs {
			proofs: Arc::new(Mutex::new(HashMap::new())),
			verification_list: Arc::new(Mutex::new(HashMap::new())),
		}
	}
}
impl EventProofs for InMemoryEventProofs {
	fn add_event_proof(&self, event: WitnessedEventRequest, origin: String) -> Result<u16, Error> {
		if let Some(stream_ref) = event.event.as_ref() {
			let event_id = stream_ref.event_id.clone();
			if self
				.verification_list
				.lock()
				.unwrap()
				.entry(event_id.clone())
				.or_insert(Vec::new())
				.contains(&origin)
			{
				log::info!("{} already sent a proof for stream {}", origin, event_id);
				Err(Error::new(ErrorKind::AlreadyExists, "Already sent a proof"))
			} else {
				self.proofs
					.lock()
					.unwrap()
					.entry(event_id.clone())
					.or_insert(Vec::new())
					.push(event.clone());
				Ok(self.get_proof_count(&event_id))
			}
		} else {
			Err(Error::new(ErrorKind::InvalidData, "Could not retreive Stream info"))
		}
	}
	fn contains(&self, id: String) -> bool {
		self.verification_list.lock().unwrap().contains_key(&id)
	}
	fn get_proof_count(&self, event_id: &str) -> u16 {
		self.proofs.lock().unwrap().get(event_id).unwrap().len() as u16
	}
}
