use std::{error::Error as E, fmt};

use sp_core::H256;
#[derive(Debug)]
pub enum Error {
	AlreadySentProof(H256),
	LockFail(String),
	BadWitnessedEventSignature(String),
	SerilizationFailure(String),
	Other(String),
}
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Error::AlreadySentProof(h) => write!(f, "Already sent proof for event_id {:?}", h),
			Error::LockFail(r) => write!(f, "failed locking ressource {}", r),
			Error::BadWitnessedEventSignature(source) =>
				write!(f, "received  bad witnessed event signature from {}", source),
			Error::SerilizationFailure(reason) =>
				write!(f, "serialization failed due to {}", reason),
			Error::Other(reason) => write!(f, "{}", reason),
		}
	}
}
impl E for Error {}
