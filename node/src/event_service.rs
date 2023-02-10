use crate::{
	event_proofs::EventProofs,
	gossip::{Order, StreamsGossip, WitnessedEvent},
	key_vault::KeyVault,
	service::FullClient,
};
use futures::channel::mpsc::Sender;
use libp2p::gossipsub::IdentTopic;
use node_runtime::{
	self,
	opaque::{Block, BlockId},
	pallet_validated_streams::ExtrinsicDetails,
};
use sc_client_api::HeaderBackend;
use sc_transaction_pool::{BasicPool, FullChainApi};
use sc_transaction_pool_api::TransactionSource;
use sp_api::ProvideRuntimeApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::{app_crypto::RuntimePublic, key_types::AURA, OpaqueExtrinsic};
use std::{
	io::{Error, ErrorKind},
	sync::Arc,
};
pub use tonic::{transport::Server, Request, Response, Status};
const TX_SOURCE: TransactionSource = TransactionSource::External;
pub struct EventService {
	target: u16,
	validators: Vec<Public>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	order_transmitter: Sender<Order>,
	keyvault: KeyVault,
	tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	client: Arc<FullClient>,
}
impl EventService {
	pub fn new(
		validators: Vec<Public>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		order_transmitter: Sender<Order>,
		keyvault: KeyVault,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
		client: Arc<FullClient>,
	) -> EventService {
		EventService {
			target: EventService::target(validators.len(), event_proofs.clone()),
			validators,
			event_proofs,
			order_transmitter,
			keyvault,
			tx_pool,
			client,
		}
	}
	pub fn target(num_peers: usize, event_proofs: Arc<dyn EventProofs + Send + Sync>) -> u16 {
		let validators_length = num_peers + 1;
		let target = (2 * ((validators_length - 1) / 3) + 1) as u16;
		event_proofs.set_target(target).ok();
		log::info!("Minimal number of nodes that needs to witness Streams is: {}", target);
		target
	}

	pub async fn handle_client_request(&self, event: H256) -> Result<String, Error> {
		let witnessed_event = self.create_witnessed_event(event).await?;
		let response = self.handle_witnessed_event(witnessed_event.clone()).await?;
		StreamsGossip::publish(
			self.order_transmitter.clone(),
			IdentTopic::new("WitnessedEvent"),
			bincode::serialize(&witnessed_event).expect("failed serializing"),
		)
		.await;
		Ok(response)
	}
	//verify that source is one of validators
	pub async fn handle_witnessed_event(
		&self,
		witnessed_event: WitnessedEvent,
	) -> Result<String, Error> {
		if self.verify_witnessed_event(&witnessed_event) {
			match self
				.event_proofs
				.add_event_proof(&witnessed_event, witnessed_event.pub_key.clone())
			{
				Ok(proof_count) => {
					log::info!("proof count is at:{}", proof_count);
					if proof_count == self.target {
						//if proof_count == 1 {
						Ok(self.submit_event_extrinsic(witnessed_event.event_id).await?.to_string())
					} else {
						Ok("Event has been added to the event proofs and proof_count increased"
							.to_string())
					}
				},
				Err(e) => {
					log::info!("Failed adding event proof due to error:{:?}", e);
					Err(Error::new(ErrorKind::Other, format!("{:?}", e)))
				},
			}
		} else {
			log::error!("bad witnessed event signature from peer {:?}", witnessed_event.pub_key);
			Err(Error::new(
				ErrorKind::Other,
				format!("bad witnessed event signature from peer {:?}", witnessed_event.pub_key),
			))
		}
	}

	//add and update the target
	#[allow(dead_code)]
	pub fn add_validator(&mut self, validator: Public) {
		self.validators.push(validator);
		let target = EventService::target(self.validators.len(), self.event_proofs.clone());
		self.target = target;
	}

	pub async fn submit_event_extrinsic(&self, event_id: H256) -> Result<H256, Error> {
		let best_block_id = BlockId::hash(self.client.info().best_hash);
		let unsigned_extrinsic = self
			.client
			.runtime_api()
			.create_unsigned_extrinsic(&best_block_id, event_id)
			.map_err(|e| Error::new(ErrorKind::Other, format!("{:?}", e)))?;
		let opaque_extrinsic = OpaqueExtrinsic::from(unsigned_extrinsic);
		self.tx_pool
			.pool()
			.submit_one(&best_block_id, TX_SOURCE, opaque_extrinsic)
			.await
			.map_err(|e| Error::new(ErrorKind::Other, format!("{:?}", e)))
	}

	pub async fn create_witnessed_event(&self, event_id: H256) -> Result<WitnessedEvent, Error> {
		match self
			.keyvault
			.keystore
			.sign_with(AURA, &self.keyvault.keys, event_id.as_bytes())
			.await
		{
			Ok(v) =>
				if let Some(sig) = v {
					Ok(WitnessedEvent {
						signature: sig,
						pub_key: self.keyvault.pubkey.to_vec(),
						event_id,
					})
				} else {
					Err(Error::new(ErrorKind::Other, "Failed retriving signature".to_string()))
				},
			Err(e) => Err(Error::new(
				ErrorKind::Other,
				format!("Could not sign Witnessed stream due to error{:?}", e),
			)),
		}
	}

	/// verifies whether the received witnessed event was originited by one of the validators
	/// than proceeds to retreiving the pubkey and the signature and checks the signature
	fn verify_witnessed_event(&self, witnessed_event: &WitnessedEvent) -> bool {
		if let Ok(pubkey) = Public::from_slice(witnessed_event.pub_key.as_slice()) {
			if self.validators.contains(&pubkey) {
				if let Some(signature) = Signature::from_slice(witnessed_event.signature.as_slice())
				{
					pubkey.verify(&witnessed_event.event_id, &signature)
				} else {
					log::error!("cant create sr25519 signature from witnessed event");
					false
				}
			} else {
				log::error!("received a gossip message from a non validator");
				false
			}
		} else {
			log::error!("cant retreive the sr25519 key from witnessed event");
			false
		}
	}
}
