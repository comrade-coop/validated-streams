use sc_consensus::{BlockCheckParams, BlockImportParams, ImportResult};
use node_runtime::{self, opaque::Block,RuntimeApi};
use std::collections::HashMap;
use sp_blockchain::well_known_cache_keys;
use sp_consensus::Error as ConsensusError;
pub use sc_executor::NativeElseWasmExecutor;
use std::sync::Arc;
use log::{debug, error, info, trace, warn};
use crate::service::ExecutorDispatch;
pub(crate) type FullClient =
	sc_service::TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;
#[derive(Clone)]
pub struct WitnessBlockImport<I>(pub I,pub Arc<sc_transaction_pool::BasicPool<sc_transaction_pool::FullChainApi<FullClient,Block>,Block>>);
#[async_trait::async_trait]
impl<I: sc_consensus::BlockImport<Block>> sc_consensus::BlockImport<Block> for WitnessBlockImport<I>  where I:Send
  {  
    type Error = ConsensusError;
    type Transaction = I::Transaction;

    async fn check_block(
        &mut self,
        block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        let parent_result = self.0.check_block(block).await;
        match parent_result
        {
            Ok(result)=> {
                info!("👌Block Checked");
                return Ok(result);}
            Err(e) => {return Err(ConsensusError::ClientImport(format!("{}",e)));
        }
    }}

    async fn import_block(
        &mut self,
        block: BlockImportParams<Block, Self::Transaction>,
        cache: HashMap<well_known_cache_keys::Id, Vec<u8>>,
    ) -> Result<ImportResult, Self::Error> {
        if let Some(block_extrinsics) = &block.body {
            // get an iterator for all ready transactions and skip the first element which contains
            // the default extrinsic
            let mut block_extrinsics = block_extrinsics.iter();
            block_extrinsics.next();
            for extrinsic in block_extrinsics
            {
                println!("Block Extrinsic:: {:?}",extrinsic);
                let mut ready_transactions= self.1.pool().validated_pool().ready();
                let is_found = ready_transactions.any(|tx| {
                    let tx_extrinsic = &tx.data;
                    if tx_extrinsic == extrinsic{
                        return true;
                    }else
                    {
                        return false;
                    };
                } );
                if is_found == false {
		            return Err(ConsensusError::ClientImport(format!("Extrinsic does not exist in the pool")));
                }
            }
        }
        let parent_result = self.0.import_block(block,cache).await;
        match parent_result
        {
            Ok(result)=> {
            info!("👌Block Imported");
                return Ok(result);}
            Err(e) => {return Err(ConsensusError::ClientImport(format!("{}",e)));
        }
    }
}
}
