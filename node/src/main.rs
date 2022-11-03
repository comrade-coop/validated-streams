//! Substrate Node Template CLI library.
#![warn(missing_docs)]
mod chain_spec;
#[macro_use]
mod service;
mod benchmarking;
mod cli;
mod command;
mod rpc;
mod streams_server;
mod witness_block_import;
use std::thread;
use streams_server::MyStreams;
fn main() -> Result<(), sc_cli::Error> {
	thread::spawn(|| {
		MyStreams::run();
	});
	command::run()
}
