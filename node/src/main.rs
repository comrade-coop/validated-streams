//! Substrate Node Template CLI library.
#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
mod service;
mod benchmarking;
mod cli;
mod command;
mod rpc;
mod witness_block_import;
fn main() -> sc_cli::Result<()> {
	command::run()
}
