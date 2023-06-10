//! Validated streams event proof types and storage

use super::{EventProofsTrait, WitnessedEvent};
use crate::errors::Error;

use sp_core::{offchain::OffchainStorage, H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::collections::HashMap;

/// persistent database for storing event proofs based on [OffchainStorage]
pub struct OffchainStorageEventProofs<Storage: OffchainStorage> {
	storage: Storage,
}

impl<Storage: OffchainStorage> OffchainStorageEventProofs<Storage> {
	/// returns a OffchainStorageEventProofs instance that persists data in the provided [Storage]
	pub fn new(storage: Storage) -> Self {
		Self { storage }
	}
}

const OFFCHAIN_PREFIX: &[u8] = b"EventProofs";

impl<Storage: OffchainStorage> EventProofsTrait for OffchainStorageEventProofs<Storage> {
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
				OFFCHAIN_PREFIX,
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
						OFFCHAIN_PREFIX,
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
					OFFCHAIN_PREFIX,
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
				if !validators.contains(pub_key) {
					self.storage.clone().remove(
						OFFCHAIN_PREFIX,
						&[event_id.as_ref(), &bincode::serialize(pub_key).unwrap()].concat(),
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
