use sc_executor::RuntimeVersionOf;
use sc_service::TFullClient;
use sp_api::BlockT;
use sp_blockchain::Info;
use sp_core::traits::CodeExecutor;

/// Extension to the substrate API, needed because it is otherwise near-impossible to refer to
/// TFullClient::chain_info generically
pub trait ChainInfo<Block: BlockT> {
	fn chain_info(&self) -> Info<Block>;
}
impl<Block: BlockT, TRtApi, TExec: CodeExecutor + RuntimeVersionOf + Clone + 'static>
	ChainInfo<Block> for TFullClient<Block, TRtApi, TExec>
{
	fn chain_info(&self) -> Info<Block> {
		<TFullClient<Block, TRtApi, TExec>>::chain_info(self)
	}
}
