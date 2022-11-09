use std::{fmt::Error, collections::HashMap};
use std::sync::{Arc, Mutex};

use crate::streams_server::validated_streams::WitnessedStream;

pub trait StreamProofs
{
    fn contains(&self,id:String) -> bool;
    fn add_stream_proof(&self,stream:WitnessedStream,origin:String) -> Result<u16,Error>;
    fn get_proof_count(&self,id:&str) -> u16;
}
pub struct InMemoryStreamProofs 
{
    target: u16, 
    proofs: Arc<Mutex<HashMap<String,Vec<String>>>>,
    verification_list: Arc<Mutex<HashMap<String,Vec<String>>>>
}
impl InMemoryStreamProofs
{
    pub fn new()-> InMemoryStreamProofs
    {
        InMemoryStreamProofs { target: 1, proofs: Arc::new(Mutex::new(HashMap::new())), verification_list: Arc::new(Mutex::new(HashMap::new()))}
    }
}
impl StreamProofs for InMemoryStreamProofs
{
    fn add_stream_proof(&self,stream:WitnessedStream,origin:String) -> Result<u16,Error> {
        if self.verification_list.lock().unwrap().entry(stream.stream_id.clone()).or_insert(Vec::new()).contains(&origin)
        {
            println!("{} already sent a proof for stream {}",origin,stream.stream_id);
        }else
        {
            self.proofs.lock().unwrap().entry(stream.stream_id.clone()).or_insert(Vec::new()).push(stream.digest);
        }
        Ok(self.get_proof_count(&stream.stream_id))   
    }
    fn contains(&self,id:String) -> bool {
        self.verification_list.lock().unwrap().contains_key(&id)
    }
    fn get_proof_count(&self,id:&str) -> u16 {
        
        self.proofs.lock().unwrap().get(id).unwrap().len() as u16
    }
}