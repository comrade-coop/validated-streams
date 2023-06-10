//! Implementation of the validated streams protocol
#![feature(async_closure)]
#![warn(missing_docs)]
pub mod configs;
pub mod errors;
pub mod events;
pub mod gossip;
pub mod node;
pub mod proofs;
pub mod server;
pub mod traits;
pub mod witness_block_import;

#[cfg(not(feature = "on-chain-proofs"))]
pub use witness_block_import::WitnessBlockImport;
