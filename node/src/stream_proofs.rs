use std::{
	collections::HashMap,
	io::{Error, ErrorKind},
	sync::{Arc, Mutex},
};

use crate::streams_server::validated_streams::WitnessedStream;

pub trait StreamProofs {
	fn contains(&self, id: String) -> bool;
	fn add_stream_proof(&self, stream: WitnessedStream, origin: String) -> Result<u16, Error>;
	fn get_proof_count(&self, id: &str) -> u16;
}

pub struct InMemoryStreamProofs {
	proofs: Arc<Mutex<HashMap<String, Vec<WitnessedStream>>>>,
	verification_list: Arc<Mutex<HashMap<String, Vec<String>>>>,
}
impl InMemoryStreamProofs {
	pub fn new() -> InMemoryStreamProofs {
		InMemoryStreamProofs {
			proofs: Arc::new(Mutex::new(HashMap::new())),
			verification_list: Arc::new(Mutex::new(HashMap::new())),
		}
	}
}
impl StreamProofs for InMemoryStreamProofs {
	fn add_stream_proof(&self, stream: WitnessedStream, origin: String) -> Result<u16, Error> {
		if let Some(stream_ref) = stream.stream.as_ref() {
			let stream_id = stream_ref.stream_id.clone();
			if self
				.verification_list
				.lock()
				.unwrap()
				.entry(stream_id.clone())
				.or_insert(Vec::new())
				.contains(&origin)
			{
				log::info!("{} already sent a proof for stream {}", origin, stream_id);
				Err(Error::new(ErrorKind::AlreadyExists, "Already sent a proof"))
			} else {
				self.proofs
					.lock()
					.unwrap()
					.entry(stream_id.clone())
					.or_insert(Vec::new())
					.push(stream.clone());
				Ok(self.get_proof_count(&stream_id))
			}
		} else {
			Err(Error::new(ErrorKind::InvalidData, "Could not retreive Stream info"))
		}
	}
	fn contains(&self, id: String) -> bool {
		self.verification_list.lock().unwrap().contains_key(&id)
	}
	fn get_proof_count(&self, id: &str) -> u16 {
		self.proofs.lock().unwrap().get(id).unwrap().len() as u16
	}
}
