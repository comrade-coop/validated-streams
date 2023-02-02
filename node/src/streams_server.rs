use crate::{event_proofs::EventProofs, gossip::StreamsGossip, service::FullClient, key_vault::KeyVault};
use futures::channel::mpsc::channel;
use local_ip_address::local_ip;
use sc_transaction_pool::{BasicPool, FullChainApi};
use node_runtime::opaque::Block;
use sp_keystore::CryptoStore;
use sp_runtime::key_types::AURA;
use crate::event_service::EventService;
use std::{sync::Arc, time::Duration};
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
	pub async fn run(event_proofs: Arc<dyn EventProofs + Send + Sync>,client: Arc<FullClient>,keystore:Arc<dyn CryptoStore>,
        tx_pool:Arc<BasicPool<FullChainApi<FullClient,Block>,Block>>)
	{
        //wait until all keys are created by aura
        tokio::time::sleep(Duration::from_millis(3000)).await;
        let keyvault = KeyVault::new(keystore, client.clone(), AURA).await;
        let (tx,rc) = channel(64);
        let events_service= Arc::new(EventService::new(KeyVault::validators_pubkeys(client.clone()),event_proofs,tx,keyvault,tx_pool,client));
        let events_service_clone = events_service.clone();
        let streams_gossip = StreamsGossip::new().await;
        streams_gossip.start(rc,events_service_clone).await;

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
