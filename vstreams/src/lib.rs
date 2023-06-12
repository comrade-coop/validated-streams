//! Implementation of the Validated Streams protocol.
//! Validated Streams is a consensus mechanism that enables a decentralized network of nodes to
//! agree on and respond to events they observe in the world around them. It empowers developers to
//! create on-chain applications that reactively source data from off-chain applications, while
//! requiring confirmation of the occurrence of off-chain events from at least two-thirds of
//! validators. See the README file (at <https://github.com/comrade-coop/validated-streams>) for more details on the architecture.

#![feature(async_closure)]
#![warn(missing_docs)]
pub mod config;
pub mod errors;
pub mod events;
pub mod gossip;
pub mod node;
pub mod proofs;
pub mod server;
pub mod traits;
pub mod block_import;

#[cfg(feature = "off-chain-proofs")]
pub use block_import::ValidatedStreamsBlockImport;

pub use config::ValidatedStreamsNetworkConfiguration;

pub use node::start;
