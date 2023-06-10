//! A sample Substrate node supporting the Validated Streams consensus primitive.
#![warn(missing_docs)]
mod chain_spec;
#[macro_use]
mod service;
mod benchmarking;
mod cli;
mod command;
mod rpc;
fn main() -> Result<(), sc_cli::Error> {
	command::run()
}
