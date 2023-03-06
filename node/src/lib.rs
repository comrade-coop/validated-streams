//! A Substrate node supporting the Validated Streams consensus primitive.
//! Validated Streams is a consensus mechanism that enables a decentralized network of nodes to
//! agree on and respond to events they observe in the world around them. It empowers developers to
//! create on-chain applications that reactively source data from off-chain applications, while
//! requiring confirmation of the occurrence of off-chain events from at least two-thirds of
//! validators. See the README file (at <https://github.com/comrade-coop/validated-streams>) for more details on the architecture.

#![warn(missing_docs)]
pub mod chain_spec;
pub mod configs;
pub mod rpc;
pub mod service;
pub mod streams;
