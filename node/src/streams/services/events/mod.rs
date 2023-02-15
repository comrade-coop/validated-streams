use crate::{
	service::FullClient,
	streams::{
		errors::Error,
		gossip::{Order, StreamsGossip},
		proofs::EventProofs,
	},
};
use sp_consensus_aura::AuraApi;
pub mod keyvault;
use futures::{channel::mpsc::Sender, StreamExt};
use keyvault::KeyVault;
use libp2p::gossipsub::IdentTopic;
use node_runtime::{
	self,
	opaque::{Block, BlockId},
	pallet_validated_streams::ExtrinsicDetails,
};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use sc_transaction_pool::{BasicPool, FullChainApi};
use sc_transaction_pool_api::TransactionSource;
use serde::{Deserialize, Serialize};
use sp_api::ProvideRuntimeApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::{app_crypto::RuntimePublic, key_types::AURA, OpaqueExtrinsic};
use std::sync::{Arc, Mutex};
pub use tonic::{transport::Server, Request, Response, Status};
const TX_SOURCE: TransactionSource = TransactionSource::Local;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WitnessedEvent {
	pub signature: Vec<u8>,
	pub pub_key: Vec<u8>,
	pub event_id: H256,
}

pub struct EventService {
	target: u16,
	validators: Arc<Mutex<Vec<Public>>>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	order_transmitter: Sender<Order>,
	keyvault: KeyVault,
	tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	client: Arc<FullClient>,
}
impl EventService {
	pub async fn new(
		validators: Vec<Public>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		order_transmitter: Sender<Order>,
		keyvault: KeyVault,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
		client: Arc<FullClient>,
	) -> EventService {
		let target = EventService::target(validators.len());
		let validators = Arc::new(Mutex::new(validators.clone()));
		EventService::handle_imported_blocks(client.clone(), validators.clone()).await;
		EventService {
			target,
			validators,
			event_proofs,
			order_transmitter,
			keyvault,
			tx_pool,
			client,
		}
	}
	pub fn target(num_validators: usize) -> u16 {
		let validators_length = num_validators + 1;
		(2 * ((validators_length - 1) / 3) + 1) as u16
	}

	pub async fn handle_client_request(&self, event: H256) -> Result<String, Error> {
		let witnessed_event = self.create_witnessed_event(event).await?;
		let response = self.handle_witnessed_event(witnessed_event.clone()).await?;
		let serilized_event = bincode::serialize(&witnessed_event)
			.map_err(|e| Error::SerilizationFailure(e.to_string()))?;
		StreamsGossip::publish(
			self.order_transmitter.clone(),
			IdentTopic::new("WitnessedEvent"),
			serilized_event,
		)
		.await;
		Ok(response)
	}
	//verify that source is one of validators
	pub async fn handle_witnessed_event(
		&self,
		witnessed_event: WitnessedEvent,
	) -> Result<String, Error> {
		if self.verify_witnessed_event(&witnessed_event)? {
			match self
				.event_proofs
				.add_event_proof(&witnessed_event, witnessed_event.pub_key.clone())
			{
				Ok(proof_count) => {
					log::info!("proof count is at:{}", proof_count);
					if proof_count == self.target {
						self.submit_event_extrinsic(witnessed_event.event_id).await?;
						Ok(format!("Event:{} has been witnessed by a mjority of validators and is in TXPool, Current Proof count:{}",witnessed_event.event_id,proof_count))
					} else {
						Ok(format!(
							"Event:{} has been added to the event proofs, Current Proof Count:{}",
							witnessed_event.event_id, proof_count
						))
					}
				},
				Err(e) => {
					log::info!("{}", e);
					Err(e)
				},
			}
		} else {
			log::error!("bad witnessed event signature from peer {:?}", witnessed_event.pub_key);
			Err(Error::BadWitnessedEventSignature("witnessed_event.pub_key".to_string()))
		}
	}

	pub async fn submit_event_extrinsic(&self, event_id: H256) -> Result<H256, Error> {
		let best_block_id = BlockId::hash(self.client.info().best_hash);
		let unsigned_extrinsic = self
			.client
			.runtime_api()
			.create_unsigned_extrinsic(&best_block_id, event_id)
			.map_err(|e| Error::Other(e.to_string()))?;
		let opaque_extrinsic = OpaqueExtrinsic::from(unsigned_extrinsic);
		self.tx_pool
			.pool()
			.submit_one(&best_block_id, TX_SOURCE, opaque_extrinsic)
			.await
			.map_err(|e| Error::Other(e.to_string()))
	}

	pub async fn update_validators(
		client: Arc<FullClient>,
		validators: Arc<Mutex<Vec<Public>>>,
	) -> Result<bool, Error> {
		let block_id = BlockId::hash(client.info().best_hash);
		let authority_ids = client
			.runtime_api()
			.authorities(&block_id)
			.map_err(|e| Error::Other(e.to_string()))?;
		*validators.lock().map_err(|_| Error::LockFail("ValidatorsList".to_string()))? =
			authority_ids
				.iter()
				.map(|pubkey| Public::from_h256(H256::from_slice(pubkey.as_slice())))
				.collect();
		Ok(true)
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
					Err(Error::Other("Failed retriving signature".to_string()))
				},
			Err(e) => Err(Error::Other(e.to_string())),
		}
	}

	/// verifies whether the received witnessed event was originited by one of the validators
	/// than proceeds to retreiving the pubkey and the signature and checks the signature
	fn verify_witnessed_event(&self, witnessed_event: &WitnessedEvent) -> Result<bool, Error> {
		let pubkey = Public::from_slice(witnessed_event.pub_key.as_slice()).map_err(|_| {
			Error::Other("cant retreive sr25519 keys from WitnessedEvent".to_string())
		})?;
		if self
			.validators
			.lock()
			.map_err(|_| Error::LockFail("ValidatorsList".to_string()))?
			.contains(&pubkey)
		{
			let signature = Signature::from_slice(witnessed_event.signature.as_slice()).ok_or(
                Error::Other("cant create sr25519 signature from witnessed event".to_string()))?;
			Ok(pubkey.verify(&witnessed_event.event_id, &signature))
		} else {
			log::error!("received a gossip message from a non validator");
			Ok(false)
		}
	}
	pub fn verify_events_validity(
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs>,
		ids: Vec<H256>,
	) -> Result<Vec<H256>, Error> {
		let best_block = BlockId::hash(client.info().best_hash);
		let authorities_len = client
			.runtime_api()
			.authorities(&best_block)
			.map_err(|e| Error::Other(e.to_string()))?
			.len();
		let target = Self::target(authorities_len);
		let mut unprepared_ids = Vec::new();
		for id in ids {
			if event_proofs.contains(id)? {
				let current_count = event_proofs.get_proof_count(id)?;
				if current_count < target {
					unprepared_ids.push(id);
				}
			} else {
				unprepared_ids.push(id);
			}
		}
		Ok(unprepared_ids)
	}
	async fn handle_imported_blocks(client: Arc<FullClient>, validators: Arc<Mutex<Vec<Public>>>) {
		tokio::spawn(async move {
			loop {
				client.import_notification_stream().select_next_some().await;
				if let Err(e) =
					EventService::update_validators(client.clone(), validators.clone()).await
				{
					log::error!("{}", e.to_string());
				}
			}
		});
	}
}
