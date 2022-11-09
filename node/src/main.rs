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
mod stream_proofs;
fn main() -> Result<(), sc_cli::Error> {
	command::run()
}
