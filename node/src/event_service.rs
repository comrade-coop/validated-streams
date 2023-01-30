use futures::channel::mpsc::Sender;
use libp2p::{Multiaddr, identity::PublicKey, gossipsub::IdentTopic};
use crate::{event_proofs::EventProofs, gossip::{StreamsGossip, Order}};
use crate::gossip::WitnessedEvent;
use subxt::{tx::SubmittableExtrinsic, OnlineClient, PolkadotConfig};
pub use tonic::{transport::Server, Request, Response, Status};
use std::sync::Arc;
use crate::streams_server::{ValidateEventRequest,ValidateEventResponse};
pub struct EventService {
	target: u16,
	validators: Vec<Multiaddr>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	streams_gossip: StreamsGossip,
	order_transmitter: Sender<Order>,
    peer_id:String
}
impl EventService {
	pub fn new(
		validators: Vec<Multiaddr>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		streams_gossip: StreamsGossip,
		order_transmitter: Sender<Order>,
	) -> EventService {
        let peer_id = streams_gossip.key.public().to_peer_id().to_base58();
		EventService {
			target: EventService::target(validators.len()),
			validators,
			event_proofs,
			streams_gossip,
			order_transmitter,
            peer_id,
		}
	}
	pub fn target(num_peers: usize) -> u16 {
		let validators_length = num_peers + 1;
		let target = (2 * ((validators_length - 1) / 3) + 1) as u16;
		log::info!("Minimal number of nodes that needs to witness Streams is: {}", target);
		target
	}
    pub async fn handle_client_request(&self,event:ValidateEventRequest)-> Result<Response<ValidateEventResponse>,Status>{
        let witnessed_event = self.create_witnessed_event(event);
        self.handle_witnessed_event(witnessed_event.clone(),self.peer_id.clone()).await;
        StreamsGossip::publish(self.order_transmitter.clone(), IdentTopic::new("WitnessedEvent"),
        bincode::serialize(&witnessed_event).expect("failed serializing")).await;
        Ok(Response::new(ValidateEventResponse { status: "event gossiped".into() }))
    }
    //verify that source is one of validators
    pub async fn handle_witnessed_event(&self,witnessed_event:WitnessedEvent,source:String)
    {
        if EventService::verify_witnessed_event(&witnessed_event)
        {
            match self.event_proofs.add_event_proof(&witnessed_event, source){
                Ok(proof_count)=>{
                    log::info!("proof count:{}",proof_count);
                    //if proof_count == self.target{
                    if proof_count == 1{
                        EventService::submit_event_extrinsic(witnessed_event.extrinsic).await;
                    }
                },
                Err(e)=>{log::info!("Failed adding event proof due to error:{:?}",e);}
            }
        }else
        {
            log::error!("bad witnessed event signature from peer {:?}",source);
        }

    }
    //add and update the target
    pub fn add_validator(_validator:Multiaddr){}

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
    pub fn create_witnessed_event(
        &self,
        event: ValidateEventRequest,
    ) -> WitnessedEvent{
        let sig = self.streams_gossip.key.sign(&event.extrinsic.as_slice()).ok().expect("failed signing extrinsic");
        WitnessedEvent{
            signature: sig,
            pub_key:self.streams_gossip.key.public().to_protobuf_encoding(),
            event_id: event.event_id,
            extrinsic: event.extrinsic
        }
    }
    // also returns false if failinf trying to retreive the pubkey from protobuf encoding 
	fn verify_witnessed_event(witnessed_event: &WitnessedEvent) -> bool {
        match &PublicKey::from_protobuf_encoding(&witnessed_event.pub_key){
            Ok(pubkey)=>PublicKey::verify(pubkey,&witnessed_event.extrinsic,&witnessed_event.signature),
            Err(e)=>{log::error!("failed decoding pubkey from witnessed event with error:{:?}",e); false},
        }
	}

}
