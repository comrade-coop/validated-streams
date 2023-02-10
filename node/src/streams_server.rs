use crate::{
	event_proofs::EventProofs, event_service::EventService, gossip::StreamsGossip,
	key_vault::KeyVault, service::FullClient,
};
use futures::channel::mpsc::channel;
use local_ip_address::local_ip;
use node_runtime::opaque::Block;
use sc_transaction_pool::{BasicPool, FullChainApi};
use sp_core::H256;
use sp_keystore::CryptoStore;
use sp_runtime::key_types::AURA;
use std::{
	io::{Error, ErrorKind},
	sync::Arc,
	time::Duration,
};
pub use tonic::{transport::Server, Request, Response, Status};
pub use validated_streams::{
	streams_server::{Streams, StreamsServer},
	ValidateEventRequest, ValidateEventResponse,
};

pub mod validated_streams {
	tonic::include_proto!("validated_streams");
}

pub struct ValidatedStreamsNode {
	events_service: Arc<EventService>,
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
			.ok_or_else(|| Status::aborted("Malformed Request, can't retreive Origin address"))?;
		log::info!("Received a request from {:?}", remote_addr);
		let event = request.into_inner();
		//double check that event_id is 32 bytes long otherwise could
		//risk panicing when creating h256 hash
		if event.event_id.len() == 32 {
			Ok(Response::new(ValidateEventResponse {
				status: self
					.events_service
					.handle_client_request(H256::from_slice(event.event_id.as_slice()))
					.await?,
			}))
		} else {
			Err(Error::new(ErrorKind::Other, "invalid event_id sent".to_string()).into())
		}
	}
}

impl ValidatedStreamsNode {
	pub async fn run(
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		client: Arc<FullClient>,
		keystore: Arc<dyn CryptoStore>,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	) {
		//wait until all keys are created by aura
		tokio::time::sleep(Duration::from_millis(3000)).await;
		if let Ok(keyvault) = KeyVault::new(keystore, client.clone(), AURA).await {
			let (tx, rc) = channel(64);
			let events_service = Arc::new(EventService::new(
				KeyVault::validators_pubkeys(client.clone()),
				event_proofs,
				tx,
				keyvault,
				tx_pool,
				client,
			));
			let events_service_clone = events_service.clone();
			let streams_gossip = StreamsGossip::new().await;
			streams_gossip.start(rc, events_service_clone).await;

			match tokio::spawn(async move {
				log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
				Server::builder()
					.add_service(StreamsServer::new(ValidatedStreamsNode { events_service }))
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
		} else {
			log::info!("node is not a validator");
		}
	}
}
