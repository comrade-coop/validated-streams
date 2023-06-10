//! Validated streams event proof types and storage

use crate::errors::Error;
use super::{WitnessedEvent, EventProofs};

use sp_core::{H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::{
	collections::{HashMap},
};

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

