//! Validated streams event proof types and storage

use crate::errors::Error;
use serde::{Deserialize, Serialize};
use sp_core::{offchain::OffchainStorage, H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::{
	collections::{hash_map::Entry, HashMap},
	sync::Mutex,
};

#[cfg(test)]
pub mod tests;

/// map event ids to their proofs
pub type ProofsMap = HashMap<H256, HashMap<CryptoTypePublicPair, Vec<u8>>>;

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
	/// adds an event proof to the given witnessed event, creating the event if it does not exist
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<(), Error>;
	/// adds all the event proofs
	fn add_events_proofs(&self, proofs: ProofsMap) -> Result<(), Error> {
		for (event_id, event_proofs) in proofs {
			for (pub_key, signature) in event_proofs {
				self.add_event_proof(&WitnessedEvent { event_id, pub_key, signature })?;
			}
		}
		Ok(())
	}

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
		let event_proofs = proofs
			.get_mut(event_id)
			.ok_or_else(|| Error::Other("Event not found".to_string()))?;
		event_proofs.retain(|k, _| validators.contains(k));
		Ok(())
	}
}

/// persistent database for storing event proofs
pub struct RocksDbEventProofs {
	// <event id (32 bytes)> <public key (serialized CryptoTypePublicPair)> -> <signature>
	db: rocksdb::DB,
}

impl RocksDbEventProofs {
	/// returns a RocksDbEventProofs instance that persists data in the provided path
	pub fn create(path: &str) -> Self {
		Self { db: rocksdb::DB::open_default(path).expect("open") }
	}

	#[cfg(test)]
	pub fn destroy(path: &str) -> Result<(), Error> {
		rocksdb::DB::destroy(&rocksdb::Options::default(), path)?;
		Ok(())
	}
}

impl EventProofs for RocksDbEventProofs {
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<(), Error> {
		self.db.put(
			[event.event_id.as_ref(), &bincode::serialize(&event.pub_key)?].concat(),
			event.signature.clone(),
		)?;
		Ok(())
	}

	fn get_event_proofs(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error> {
		// NOTE: to get all proofs, no matter who signed them:
		// self.db.prefix_iterator(event_id).map(|r| { r.map(|(key, signature)| { let pub_key =
		// bincode::deserialize(&key[H256::len_bytes()..]).unwrap(); (pub_key, signature.into())
		// }).map_err(|e| e.into())}).collect()

		let values =
			self.db.multi_get(validators.iter().map(|pub_key| {
				[event_id.as_ref(), &bincode::serialize(pub_key).unwrap()].concat()
			}));
		validators
			.iter()
			.zip(values)
			.flat_map(|(pub_key, signature_r)| match signature_r {
				Ok(Some(signature)) => Some(Ok((pub_key.clone(), signature))),
				Ok(None) => None,
				Err(e) => Some(Err(e.into())),
			})
			.collect()
	}

	fn get_event_proof_count(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<u16, Error> {
		Ok(self
			.db
			.multi_get(
				validators.iter().map(|pub_key| {
					[event_id.as_ref(), &bincode::serialize(pub_key).unwrap()].concat()
				}),
			)
			.into_iter()
			.filter(|r| matches!(r, Ok(Some(_))))
			.count() as u16)
	}

	fn purge_event_stale_signatures(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<(), Error> {
		for r in self.db.prefix_iterator(event_id) {
			let (key, _signature) = r?;
			let pub_key = bincode::deserialize(&key[H256::len_bytes()..])?;
			if !validators.contains(&pub_key) {
				self.db.delete(key)?;
			}
		}
		Ok(())
	}
}

/// persistent database for storing event proofs based on [OffchainStorage]
pub struct OffchainStorageEventProofs<Storage: OffchainStorage> {
	storage: Storage,
}

impl<Storage: OffchainStorage> OffchainStorageEventProofs<Storage> {
	/// returns a OffchainStorageEventProofs instance that persists data in the provided path
	pub fn create(storage: Storage) -> Self {
		Self { storage }
	}
}

const OFFCHAIN_PREFIX: &[u8] = b"EventProofs";

impl<Storage: OffchainStorage> EventProofs for OffchainStorageEventProofs<Storage> {
	fn add_event_proof(&self, event: &WitnessedEvent) -> Result<(), Error> {
		self.storage.clone().set(
			OFFCHAIN_PREFIX,
			&[event.event_id.as_ref(), &bincode::serialize(&event.pub_key)?].concat(),
			&event.signature,
		);

		loop {
			let existing_bytes = self.storage.get(OFFCHAIN_PREFIX, event.event_id.as_ref());
			let mut signers_list = existing_bytes
				.as_ref()
				.map(|b| bincode::deserialize::<Vec<CryptoTypePublicPair>>(b.as_ref()))
				.unwrap_or_else(|| Ok(vec![]))?;
			signers_list.push(event.pub_key.clone());
			if self.storage.clone().compare_and_set(
				&OFFCHAIN_PREFIX,
				event.event_id.as_ref(),
				existing_bytes.as_ref().map(|x| x.as_ref()),
				&bincode::serialize(&signers_list)?,
			) {
				break
			}
		}
		Ok(())
	}

	fn get_event_proofs(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<HashMap<CryptoTypePublicPair, Vec<u8>>, Error> {
		Ok(validators
			.iter()
			.flat_map(|pub_key| {
				self.storage
					.get(
						&OFFCHAIN_PREFIX,
						&[event_id.as_ref(), &bincode::serialize(pub_key).unwrap()].concat(),
					)
					.map(|signature| (pub_key.clone(), signature))
			})
			.collect())
	}

	fn get_event_proof_count(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<u16, Error> {
		Ok(validators
			.iter()
			.map(|pub_key| {
				self.storage.get(
					&OFFCHAIN_PREFIX,
					&[event_id.as_ref(), &bincode::serialize(pub_key).unwrap()].concat(),
				)
			})
			.filter(|r| matches!(r, Some(_)))
			.count() as u16)
	}

	fn purge_event_stale_signatures(
		&self,
		event_id: &H256,
		validators: &[CryptoTypePublicPair],
	) -> Result<(), Error> {
		loop {
			let existing_bytes = self.storage.get(OFFCHAIN_PREFIX, event_id.as_ref());
			let mut signers_list = existing_bytes
				.as_ref()
				.map(|b| bincode::deserialize::<Vec<CryptoTypePublicPair>>(b.as_ref()))
				.unwrap_or_else(|| Ok(vec![]))?;

			signers_list.retain(|pub_key| {
				if !validators.contains(&pub_key) {
					self.storage.clone().remove(
						OFFCHAIN_PREFIX,
						&[event_id.as_ref(), &bincode::serialize(&pub_key).unwrap()].concat(),
					);
					false
				} else {
					true
				}
			});

			if self.storage.clone().compare_and_set(
				OFFCHAIN_PREFIX,
				event_id.as_ref(),
				existing_bytes.as_ref().map(|x| x.as_ref()),
				&bincode::serialize(&signers_list)?,
			) {
				break
			}
		}
		Ok(())
	}
}
