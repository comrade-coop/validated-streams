use futures::channel::mpsc::Sender;
use libp2p::gossipsub::IdentTopic;
use sp_core::{sr25519::{Public, Signature}, ByteArray};
use sp_runtime::{key_types::AURA, app_crypto::RuntimePublic};
use crate::{event_proofs::EventProofs, gossip::{StreamsGossip, Order}, key_vault::KeyVault};
use crate::gossip::WitnessedEvent;
use subxt::{tx::SubmittableExtrinsic, OnlineClient, PolkadotConfig};
pub use tonic::{transport::Server, Request, Response, Status};
use std::sync::Arc;
use crate::streams_server::{ValidateEventRequest,ValidateEventResponse};
pub struct EventService {
	target: u16,
	validators: Vec<Public>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	order_transmitter: Sender<Order>,
    keyvault:KeyVault,    
}
impl EventService {
	pub fn new(
		validators: Vec<Public>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		order_transmitter: Sender<Order>,
	    keyvault:KeyVault,
        ) -> EventService {
		EventService {
			target: EventService::target(validators.len()),
			validators,
			event_proofs,
			order_transmitter,
            keyvault,
		}
	}
	pub fn target(num_peers: usize) -> u16 {
		let validators_length = num_peers + 1;
		let target = (2 * ((validators_length - 1) / 3) + 1) as u16;
		log::info!("Minimal number of nodes that needs to witness Streams is: {}", target);
		target
	}
    pub async fn handle_client_request(&self,event:ValidateEventRequest)-> Result<Response<ValidateEventResponse>,Status>{
        let witnessed_event = self.create_witnessed_event(event).await?;
        self.handle_witnessed_event(witnessed_event.clone()).await;
        StreamsGossip::publish(self.order_transmitter.clone(), IdentTopic::new("WitnessedEvent"),
        bincode::serialize(&witnessed_event).expect("failed serializing")).await;
        Ok(Response::new(ValidateEventResponse { status: "event gossiped".into() }))
    }
    //verify that source is one of validators
    pub async fn handle_witnessed_event(&self,witnessed_event:WitnessedEvent)
    {
        if self.verify_witnessed_event(&witnessed_event)
        {
            match self.event_proofs.add_event_proof(&witnessed_event,witnessed_event.pub_key.clone()){
                Ok(proof_count)=>{
                    //if proof_count == self.target{
                    if proof_count == 1{
                        EventService::submit_event_extrinsic(witnessed_event.extrinsic).await;
                    }
                },
                Err(e)=>{log::info!("Failed adding event proof due to error:{:?}",e);}
            }
        }else
        {
            log::error!("bad witnessed event signature from peer {:?}",witnessed_event.pub_key);
        }

    }
    //add and update the target
    pub fn add_validator(&mut self,validator:Public){
        self.validators.push(validator);
        let target = EventService::target(self.validators.len());
        self.target= target;
    }

	pub async fn submit_event_extrinsic(extrinsic: Vec<u8>){
		if let Ok(api) = OnlineClient::<PolkadotConfig>::new()
            .await{
                let submitable_stream = SubmittableExtrinsic::from_bytes(api, extrinsic);
                match submitable_stream.submit().await {
                    Ok(h) => log::info!("event added on chain via tx with hash:{:?}",h),
                    Err(e) =>log::error!("Failed submitting event to the txpool with Error {:?}", e.to_string()),}
            }else{log::error!("failed creating substrate client");
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
