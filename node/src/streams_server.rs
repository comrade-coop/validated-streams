use crate::{event_proofs::EventProofs, network_configs::LocalNetworkConfiguration, gossip::StreamsGossip};
use futures::channel::mpsc::channel;
use libp2p::gossipsub::IdentTopic;
use local_ip_address::local_ip;
use crate::event_service::EventService;
use std::sync::Arc;
pub use tonic::{transport::Server, Request, Response, Status};
pub use validated_streams::{
	streams_server::{Streams, StreamsServer},
	ValidateEventRequest, ValidateEventResponse,
};

#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams {
	tonic::include_proto!("validated_streams");
}

pub struct ValidatedStreamsNode {
    events_service:Arc<EventService>
}

#[tonic::async_trait]
impl Streams for ValidatedStreamsNode {
	//check if the watcher(client) has already submitted the stream
	//if not create a WitnessedEvent message, add it to the stream proofs and gossip it
	async fn validate_event(
		&self,
		request: Request<ValidateEventRequest>,
	) -> Result<Response<ValidateEventResponse>, Status> {
		let remote_addr = request
			.remote_addr()
			.ok_or(Status::aborted("Malformed Request, can't retreive Origin address"))?;
		log::info!("Received a request from {:?}", remote_addr);
		let event = request.into_inner();
	    self.events_service.handle_client_request(event).await
    }
}

impl ValidatedStreamsNode 
{
	//could prossibly make use of node configs in the future from runner in command.rs
	pub async fn run(event_proofs: Arc<dyn EventProofs + Send + Sync>)
	{
		let self_addr = LocalNetworkConfiguration::self_multi_addr();
		let validators = LocalNetworkConfiguration::validators_multiaddrs();
        let peers = LocalNetworkConfiguration::peers_multiaddrs(self_addr.clone());
        let streams_gossip = StreamsGossip::new().await;
        streams_gossip.listen(self_addr).await;
        streams_gossip.dial_peers(peers.clone()).await;
        streams_gossip.subscribe(IdentTopic::new("WitnessedEvent")).await;
        let (tx,rc) = channel(64);
        let swarm_clone = streams_gossip.swarm.clone();
        let events_service= Arc::new(EventService::new(validators,event_proofs,streams_gossip,tx));
        let events_service_clone = events_service.clone();
        tokio::spawn(async move{
            StreamsGossip::handle_incoming_messages(swarm_clone,rc,events_service_clone).await;
        });
        match tokio::spawn(async move {
			log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
			Server::builder()
				.add_service(StreamsServer::new(ValidatedStreamsNode{events_service}))
				.serve("[::0]:5555".parse().expect("Failed parsing gRPC server Address"))
				.await
		})
		.await
		{
			Ok(_) => (),
			Err(e) => {
				panic!("Failed Creating StreamsServer due to Err: {}", e);
			},
		}
	}
}
