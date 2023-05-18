use libp2p::Multiaddr;
#[derive(Debug, clap::Parser)]
pub struct RunCmd {
	#[clap(flatten)]
	pub base: sc_cli::RunCmd,
	#[clap(long, default_value_t = 5555, value_parser = validate_port)]
	/// grpc port for the current validated streams node
	pub grpc_port: u16,
	#[clap(long)]
	/// multiaddresses of the boot nodes
	pub peers_multiaddr: Vec<Multiaddr>,
	#[clap(long, default_value = "/tmp/ProofStore")]
	/// path for the event proofs database
	pub proofs_path: String,
}

fn validate_port(p: &str) -> Result<u16, String> {
	let port = p.parse::<u16>().map_err(|_| "Invalid port number")?;
	if port >= 1024 {
		Ok(port)
	} else {
		Err("Port number must be between 1024 and 65535".to_owned())
	}
}
#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[clap(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: RunCmd,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
	/// Key management cli utilities
	#[clap(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[clap(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Try some command against runtime state.
	#[cfg(feature = "try-runtime")]
	TryRuntime(try_runtime_cli::TryRuntimeCmd),

	/// Try some command against runtime state. Note: `try-runtime` feature must be enabled.
	#[cfg(not(feature = "try-runtime"))]
	TryRuntime,

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}
