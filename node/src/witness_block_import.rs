use sc_consensus::{BlockCheckParams, BlockImportParams, ImportResult};
use node_runtime::{self, opaque::Block};
use std::collections::HashMap;
use sp_blockchain::well_known_cache_keys;
use sp_consensus::Error as ConsensusError;
#[derive(Clone)]
pub struct WitnessBlockImport<I>(pub I);
#[async_trait::async_trait]
impl<I: sc_consensus::BlockImport<Block>> sc_consensus::BlockImport<Block> for WitnessBlockImport<I>  where I:Send
  {  
    type Error = ConsensusError;
    type Transaction = I::Transaction;

    async fn check_block(
        &mut self,
        block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
            println!("Block Checked");
        let parent_result = self.0.check_block(block).await;
        match parent_result
        {
            Ok(result)=> {return Ok(result);}
            Err(e) => {return Err(ConsensusError::ClientImport(format!("{}",e)));
        }
    }}

    async fn import_block(
        &mut self,
        block: BlockImportParams<Block, Self::Transaction>,
        cache: HashMap<well_known_cache_keys::Id, Vec<u8>>,
    ) -> Result<ImportResult, Self::Error> {
        if let Some(transactions) = &block.body {
             // Do whatever check you need with the transactions
             println!("checking transactions {:?}",transactions);
        }
		//return Err(ConsensusError::ClientImport(format!("Extrinsic does not exist in the pool")));
        let parent_result = self.0.import_block(block,cache).await;
        match parent_result
        {
            Ok(result)=> {
        println!("Block Imported");
                return Ok(result);}
            Err(e) => {return Err(ConsensusError::ClientImport(format!("{}",e)));
        }
    }
}
}
