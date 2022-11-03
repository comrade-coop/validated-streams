use substrate_build_script_utils::{generate_cargo_keys, rerun_if_git_head_changed};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	tonic_build::compile_protos("../proto/streams.proto")?;
	generate_cargo_keys();

	rerun_if_git_head_changed();
	Ok(())
}
