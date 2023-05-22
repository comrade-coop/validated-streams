//! Error types.
use std::{error::Error as E, fmt};

/// An Error that has occurred during Validated Streams operation
#[derive(Debug, PartialEq)]
pub enum Error {
	/// We failed to lock a mutex or similar
	LockFail(String),
	/// The client submitted an incorrect signature
	BadWitnessedEventSignature(String),
	/// We failed to serialize a message
	SerilizationFailure(String),
	/// A database-related error
	Database(String),
	/// Any other error
	Other(String),
}
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Error::LockFail(r) => write!(f, "Failed locking resource {r}"),
			Error::BadWitnessedEventSignature(source) =>
				write!(f, "Received bad witnessed event signature from {source}"),
			Error::SerilizationFailure(reason) => write!(f, "Serialization failed due to {reason}"),
			Error::Database(reason) => write!(f, "Database error, {reason}"),
			Error::Other(reason) => write!(f, "{reason}"),
		}
	}
}
impl E for Error {}

#[doc(hidden)] // Enable use of `?` operator.
impl<T> From<std::sync::PoisonError<T>> for Error {
	fn from(e: std::sync::PoisonError<T>) -> Error {
		Error::LockFail(format!("PoisonError: {e}"))
	}
}
#[doc(hidden)] // Enable use of `?` operator.
impl From<rocksdb::Error> for Error {
	fn from(e: rocksdb::Error) -> Error {
		Error::Database(e.into_string())
	}
}
#[doc(hidden)] // Enable use of `?` operator.
impl From<Box<bincode::ErrorKind>> for Error {
	fn from(e: Box<bincode::ErrorKind>) -> Error {
		Error::SerilizationFailure(format!("{e}"))
	}
}
