//! Error types for the Validated Streams library.

use std::{error::Error as E, fmt};

/// An error which has occurred during Validated Streams operation.
#[derive(Debug, PartialEq)]
pub enum Error {
	/// We failed to lock a mutex or similar
	LockFail(String),
	/// The client submitted an incorrect signature
	BadWitnessedEventSignature(String),
	/// We failed to serialize a message
	SerilizationFailure(String),
	/// We failed to sign a message
	SigningFailure(String),
	/// A database-related error
	Database(String),
	/// The current node is not a validator
	NotAValidator,
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
			Error::SigningFailure(reason) => write!(f, "Signing failed due to {reason}"),
			Error::Database(reason) => write!(f, "Database error, {reason}"),
			Error::NotAValidator => write!(f, "Not a validator"),
			Error::Other(reason) => write!(f, "{reason}"),
		}
	}
}
impl E for Error {}

#[doc(hidden)] // Enable use of `?` operator.
impl From<Box<bincode::ErrorKind>> for Error {
	fn from(e: Box<bincode::ErrorKind>) -> Error {
		Error::SerilizationFailure(format!("{e}"))
	}
}

#[doc(hidden)] // Enable use of `?` operator.
impl From<sp_keystore::Error> for Error {
	fn from(e: sp_keystore::Error) -> Error {
		Error::SigningFailure(format!("{e}"))
	}
}

#[doc(hidden)] // Enable use of `?` operator.
impl From<sp_api::ApiError> for Error {
	fn from(e: sp_api::ApiError) -> Error {
		Error::Other(format!("{e}"))
	}
}

#[doc(hidden)] // Enable use of `?` operator.
impl<T> From<std::sync::PoisonError<T>> for Error {
	fn from(e: std::sync::PoisonError<T>) -> Error {
		Error::LockFail(format!("PoisonError: {e}"))
	}
}

#[cfg(feature = "rocksdb")]
#[doc(hidden)] // Enable use of `?` operator.
impl From<rocksdb::Error> for Error {
	fn from(e: rocksdb::Error) -> Error {
		Error::Database(e.into_string())
	}
}
