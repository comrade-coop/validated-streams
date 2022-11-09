//! Substrate Node Template CLI library.
#![warn(missing_docs)]
mod chain_spec;
#[macro_use]
mod service;
mod benchmarking;
mod cli;
mod command;
mod network_configs;
mod rpc;
mod streams_server;
mod witness_block_import;
use network_configs::LocalDockerNetworkConfiguration;
use std::thread;
use streams_server::ValidatedStreamsNode;
fn main() -> Result<(), sc_cli::Error> {
	command::run()
}
