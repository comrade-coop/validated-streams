//! Error types.
use sp_core::H256;
use std::{error::Error as E, fmt};

/// An Error that has occured during Validated Streams operation
#[derive(Debug, PartialEq)]
pub enum Error {
	/// A peer has already sent us a proof for the included event id.
	AlreadySentProof(H256),
	/// We failed to lock a mutex or similar
	LockFail(String),
	/// The client submitted an incorrect signature
	#[allow(dead_code)]
	BadWitnessedEventSignature(String),
	/// We failed to serialize a message
	SerilizationFailure(String),
	/// Any other error
	Other(String),
}
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Error::AlreadySentProof(h) =>
				write!(f, "Peer already sent a proof for event_id {:?}", h),
			Error::LockFail(r) => write!(f, "Failed locking resource {}", r),
			Error::BadWitnessedEventSignature(source) =>
				write!(f, "Received bad witnessed event signature from {}", source),
			Error::SerilizationFailure(reason) =>
				write!(f, "Serialization failed due to {}", reason),
			Error::Other(reason) => write!(f, "{}", reason),
		}
	}
}
impl E for Error {}

#[doc(hidden)] // Enable use of `?` operator.
impl<T> From<std::sync::PoisonError<T>> for Error {
	fn from(e: std::sync::PoisonError<T>) -> Error {
		Error::LockFail(format!("PoisonError: {}", e))
	}
}
