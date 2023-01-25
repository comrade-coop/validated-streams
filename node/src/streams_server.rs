use self::validated_streams::{streams_client::StreamsClient, WitnessedEventResponse};
use crate::{event_proofs::EventProofs, network_configs::NetworkConfiguration};
use futures::lock::Mutex;
use local_ip_address::local_ip;
use sp_core::{sr25519::Pair, H256};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, KeyTypeId};
use std::{
	io::{Error, ErrorKind},
	str::FromStr,
	sync::Arc,
	thread,
	time::Duration,
};
use subxt::{tx::SubmittableExtrinsic, OnlineClient, PolkadotConfig};
use tonic::transport::{Channel, Endpoint};
pub use tonic::{transport::Server, Request, Response, Status};
use validated_streams::{
	streams_server::{Streams, StreamsServer},
	ValidateEventRequest, ValidateEventResponse, WitnessedEventRequest,
};

#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams {
	tonic::include_proto!("validated_streams");
}

pub struct ValidatedStreamsNode {
	target: u16,
	peers: Vec<Endpoint>,
	validators_connections: Arc<Mutex<Vec<StreamsClient<Channel>>>>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	keystore: Arc<dyn CryptoStore>,
	key_type: KeyTypeId,
	pub_key: sp_core::sr25519::Public,
}

#[tonic::async_trait]
impl Streams for ValidatedStreamsNode {
	//check if the watcher(client) has already submitted the stream
	//if not create a WitnessedEventRequest message, add it to the stream proofs and gossip it
	async fn validate_event(
		&self,
		request: Request<ValidateEventRequest>,
	) -> Result<Response<ValidateEventResponse>, Status> {
		let remote_addr = request
			.remote_addr()
			.ok_or(Status::aborted("Malformed Request, can't retreive Origin address"))?;
		log::info!("Received a request from {:?}", remote_addr);
		let event = request.into_inner();
		let witnessed_event = self.create_witnessed_event(event).await?;
		self.verify_witnessed_event(witnessed_event.clone())?;
		let status = self
			.process_witness_result(
				self.event_proofs
					.add_event_proof(witnessed_event.clone(), remote_addr.to_string()),
				witnessed_event.clone(),
			)
			.await?;

		match ValidatedStreamsNode::gossip(self.validators_connections.clone(), witnessed_event)
			.await
		{
			Ok(_) => Ok(Response::new(ValidateEventResponse { status: status.into_inner().reply })),
			_ => Err(Status::aborted("Could not gossip the received event")),
		}
	}
	//receive what other validators have witnessed
	async fn witnessed_event(
		&self,
		request: Request<WitnessedEventRequest>,
	) -> Result<Response<WitnessedEventResponse>, Status> {
		//check signature, call add_event_proof
		let remote_addr = request
			.remote_addr()
			.ok_or(Status::aborted("Malformed Request, can't retreive Origin address"))?;
		let witnessed_event = request.into_inner();
		log::info!("Received a request from {:?}", remote_addr);
		self.verify_witnessed_event(witnessed_event.clone())?;
		self.process_witness_result(
			self.event_proofs
				.add_event_proof(witnessed_event.clone(), remote_addr.to_string()),
			witnessed_event,
		)
		.await
	}
}
impl ValidatedStreamsNode {
	pub async fn new(
		peers: Vec<Endpoint>,
		keystore: Arc<dyn CryptoStore>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> ValidatedStreamsNode {
		let target = ValidatedStreamsNode::get_target(peers.len());
		let key_type = sp_core::crypto::key_types::AURA;
		event_proofs.set_target(1).unwrap();
		if let Some(pub_key) = keystore.sr25519_generate_new(key_type, None).await.ok() {
			// let pub_key = keystore.keys(key_type).await.expect("Failed retreiving public keys
			// from keystore").get(0).expect("Failed unwraping retreived key").clone();
			ValidatedStreamsNode {
				peers,
				validators_connections: Arc::new(Mutex::new(Vec::new())),
				event_proofs,
				target,
				keystore,
				key_type,
				pub_key,
			}
		} else {
			panic!("failed creating key pair from the provided keystore")
		}
	}

	pub fn get_target(num_peers: usize) -> u16 {
		let validators_length = num_peers + 1;
		let target = (2 * ((validators_length - 1) / 3) + 1) as u16;
		log::info!("Minimal number of nodes that needs to witness Streams is: {}", target);
		target
	}

	pub fn verify_witnessed_event(
		&self,
		witnessed_event: WitnessedEventRequest,
	) -> Result<bool, Status> {
		let event = witnessed_event
			.event
			.ok_or(Status::aborted("Could not retreive the event from witnessed event"))?;
		let sig = sp_core::sr25519::Signature::from_slice(&witnessed_event.signature)
			.ok_or(Status::aborted("invalid signature given"))?;
		if let Some(pub_key) = sp_core::sr25519::Public::from_str(&witnessed_event.pub_key).ok() {
			Ok(Pair::verify_deprecated(&sig, event.extrinsic, &pub_key))
		} else {
			Err(Status::aborted("invalid public key given"))
		}
	}
	pub async fn process_witness_result(
		&self,
		result: Result<u16, Error>,
		proof: WitnessedEventRequest,
	) -> Result<Response<WitnessedEventResponse>, Status> {
		match result {
			Ok(count) => {
				let mut reply = WitnessedEventResponse { reply: String::from("") };
				// if count == self.target
				if count == 1 {
					let extrinsic = proof
						.event
						.ok_or(Status::invalid_argument("failed unwraping event extrinsic"))?
						.extrinsic;
					let hash = ValidatedStreamsNode::submit_event_extrinsic(extrinsic).await?;
					reply.reply = format!(
						"Event extrinsic has been submitted to the pool with hash {:?}",
						hash
					);
					Ok(Response::new(reply))
				} else {
					reply.reply = "Proof Count increased".to_string();
					Ok(Response::new(reply))
				}
			},
			Err(e) => Err(Status::already_exists(e.to_string())),
		}
	}
	pub async fn submit_event_extrinsic(extrinsic: Vec<u8>) -> Result<H256, Error> {
		let api = OnlineClient::<PolkadotConfig>::new()
			.await
			.or(Err(Error::new(ErrorKind::Other, "failed creating substrate client")))?;
		let submitable_stream = SubmittableExtrinsic::from_bytes(api, extrinsic);
		match submitable_stream.submit().await {
			Ok(v) => Ok(v),
			Err(e) => Err(Error::new(
				ErrorKind::Other,
				format!("Failed submitting event to the txpool with Error {}", e.to_string()),
			)),
		}
	}
	pub async fn create_witnessed_event(
		&self,
		event: ValidateEventRequest,
	) -> Result<WitnessedEventRequest, Status> {
		let stringfied_key = self.pub_key.to_string();
		let key = CryptoTypePublicPair::from(&self.pub_key);
		match self.keystore.sign_with(self.key_type, &key, event.extrinsic.as_slice()).await {
			Ok(v) =>
				if let Some(sig) = v {
					Ok(WitnessedEventRequest {
						signature: sig,
						pub_key: stringfied_key,
						event: Some(event.clone()),
					})
				} else {
					Err(Status::aborted("Failed retriving signature"))
				},
			Err(_) => Err(Status::aborted("Could not sign Witnessed stream")),
		}
	}
	pub async fn intialize_mesh_network(&mut self) {
		let connections = self.validators_connections.clone();
		let peers = self.peers.clone();
		tokio::spawn(async move {
			log::info!("waiting server to get started");
			thread::sleep(Duration::from_millis(4000));
			for addr in peers {
				let connection_result =
					StreamsClient::<tonic::transport::Channel>::connect(addr.clone()).await;
				match connection_result {
					Ok(conn) => {
						log::info!("🤜🤛Connected successfully to validator");
						connections.lock().await.push(conn);
					},
					Err(e) => {
						log::error!("failed connecting to address {:?} with error {:?}", addr, e);
					},
				}
			}
		});
	}
	pub async fn gossip(
		connections: Arc<Mutex<Vec<StreamsClient<Channel>>>>,
		event: WitnessedEventRequest,
	) -> Result<(), tonic::transport::Error> {
		for conn in &mut connections.lock().await.iter_mut() {
			let reply = conn.witnessed_event(Request::new(event.clone())).await;
			match reply {
				Ok(client_reply) => log::info!("{:?}", client_reply),
				Err(e) => {
					log::info!("failed sending witnessing stream with err {:?}", e);
				},
			}
		}
		Ok(())
	}
	//could prossibly make use of node configs in the future from runner in command.rs
	pub async fn run<T>(
		configs: T,
		keystore: Arc<dyn CryptoStore>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) where
		T: NetworkConfiguration,
	{
		let addr = configs.get_self_address();
		let self_endpoint = format!("http://{}", addr);
		let peers = configs.get_peers_addresses();
		let mut target: Vec<Endpoint> = Vec::new();
		for peer in peers.iter() {
			if *peer != self_endpoint {
				target.push(peer.clone().parse().expect("invalid Endpoint"));
			}
		}
		let mut streams = ValidatedStreamsNode::new(target, keystore, event_proofs).await;
		streams.intialize_mesh_network().await;
		match tokio::spawn(async move {
			log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
			Server::builder()
				.add_service(StreamsServer::new(streams))
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
