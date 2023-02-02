use futures::channel::mpsc::Sender;
use libp2p::gossipsub::IdentTopic;
use sc_transaction_pool::{BasicPool, FullChainApi};
use sc_transaction_pool_api::TransactionSource;
use sp_core::{sr25519::{Public, Signature}, ByteArray, H256};
use sp_runtime::{key_types::AURA, app_crypto::RuntimePublic, OpaqueExtrinsic};
use crate::{event_proofs::EventProofs, gossip::{StreamsGossip, Order}, key_vault::KeyVault, service::FullClient};
use crate::gossip::WitnessedEvent;
use node_runtime::opaque::{Block, BlockId};
use sc_client_api::HeaderBackend;
pub use tonic::{transport::Server, Request, Response, Status};
use std::{sync::Arc, io::ErrorKind};
use std::io::Error;
use crate::streams_server::{ValidateEventRequest,ValidateEventResponse};
const TX_SOURCE: TransactionSource = TransactionSource::External;
pub struct EventService {
	target: u16,
	validators: Vec<Public>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	order_transmitter: Sender<Order>,
    keyvault:KeyVault,
    tx_pool:Arc<BasicPool<FullChainApi<FullClient,Block>,Block>>,
    client:Arc<FullClient>
}
impl EventService {
	pub fn new(
		validators: Vec<Public>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		order_transmitter: Sender<Order>,
	    keyvault:KeyVault,
        tx_pool:Arc<BasicPool<FullChainApi<FullClient,Block>,Block>>,
    client:Arc<FullClient>
        ) -> EventService {
		EventService {
			target: EventService::target(validators.len(),event_proofs.clone()),
			validators,
			event_proofs,
			order_transmitter,
            keyvault,
            tx_pool,
            client
		}
	}
	pub fn target(num_peers: usize,_event_proofs:Arc<dyn EventProofs+ Send + Sync>) -> u16 {
		let validators_length = num_peers + 1;
		let target = (2 * ((validators_length - 1) / 3) + 1) as u16;
        //event_proofs.set_target(target).ok();
		log::info!("Minimal number of nodes that needs to witness Streams is: {}", target);
		target
	}
    //should return a final response of whether its included in block or not (should not care about
    //internal event handling)
    pub async fn handle_client_request(&self,event:ValidateEventRequest)-> Result<Response<ValidateEventResponse>,Status>{
        let witnessed_event = self.create_witnessed_event(event).await?;
        self.handle_witnessed_event(witnessed_event.clone()).await?;
        StreamsGossip::publish(self.order_transmitter.clone(), IdentTopic::new("WitnessedEvent"),
        bincode::serialize(&witnessed_event).expect("failed serializing")).await;
        Ok(Response::new(ValidateEventResponse { status: "event gossiped".into() }))
    }
    //verify that source is one of validators
    pub async fn handle_witnessed_event(&self,witnessed_event:WitnessedEvent)->Result<String,Error>
    {
        if self.verify_witnessed_event(&witnessed_event)
        {
            match self.event_proofs.add_event_proof(&witnessed_event,witnessed_event.pub_key.clone()){
                Ok(proof_count)=>{
                    //if proof_count == self.target{
                    if proof_count == 1{
                        Ok(self.submit_event_extrinsic(witnessed_event.extrinsic).await?.to_string())
                    }else{
                        Ok("Event has been added to the event proofs and proof_count increased".to_string())
                    }
                },
                Err(e)=>{log::info!("Failed adding event proof due to error:{:?}",e);
                    Err(Error::new(ErrorKind::Other, format!("{:?}",e)))
                }
            }
        }else
        {
            log::error!("bad witnessed event signature from peer {:?}",witnessed_event.pub_key);
            Err(Error::new(ErrorKind::Other,format!("bad witnessed event signature from peer {:?}",witnessed_event.pub_key)))
        }

    }
    //add and update the target
    pub fn add_validator(&mut self,validator:Public){
        self.validators.push(validator);
        let target = EventService::target(self.validators.len(),self.event_proofs.clone());
        self.target= target;
    }

	pub async fn submit_event_extrinsic(&self,extrinsic: Vec<u8>)-> Result<H256,Error>{
        match OpaqueExtrinsic::from_bytes(extrinsic.as_slice()){
            Ok(opaque_extrinsic)=>
            {
                let best_block_id = BlockId::hash(self.client.info().best_hash);
                return Ok(self.tx_pool.pool().submit_one(&best_block_id, TX_SOURCE,opaque_extrinsic).await.or_else(|e|
                    {Err(Error::new(ErrorKind::Other,format!("{:?}",e)))})?)
            },
            Err(e)=>{log::error!("failed creating opaque Exrtinisc from given extrinsic due to error:{:?}",e);
                    Err(Error::new(ErrorKind::Other,format!("{:?}",e)))
            },
        }    
    }

    pub async fn create_witnessed_event(
        &self,
        event: ValidateEventRequest,
    ) -> Result<WitnessedEvent,Status>{
        	match self.keyvault.keystore.sign_with(AURA, &self.keyvault.keys, event.extrinsic.as_slice()).await {
			Ok(v) =>
				if let Some(sig) = v {
                    Ok(WitnessedEvent{
                        signature: sig,
                        pub_key:self.keyvault.pubkey.to_vec(),
                        event_id: event.event_id,
                        extrinsic: event.extrinsic
                    })
				}else{
					Err(Status::aborted("Failed retriving signature"))
				},
			Err(e) => Err(Status::aborted(format!("Could not sign Witnessed stream due to error{:?}",e))),
		}            
    }
	
/// verifies whether the received witnessed event was originited by one of the validators
/// than proceeds to retreiving the pubkey and the signature and checks the signature
fn verify_witnessed_event(&self,witnessed_event: &WitnessedEvent) -> bool {
    if let Ok(pubkey) = Public::from_slice(&witnessed_event.pub_key.as_slice()) {
        if self.validators.contains(&pubkey){
            if let Some(signature) = Signature::from_slice(&witnessed_event.signature.as_slice()) {
                return pubkey.verify(&witnessed_event.extrinsic, &signature);
            } else {
                log::error!("cant create sr25519 signature from witnessed event");
                return false;
            }
        }else{
            log::error!("received a gossip message from a non validator");
            return false
        }
    } else {
        log::error!("cant retreive the sr25519 key from witnessed event");
        return false;
    }
}
}
