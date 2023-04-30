//! Service which handles incoming events from the trusted client and other nodes

use crate::{
    errors::Error,
    gossip::{StreamsGossip, StreamsGossipHandler},
    proofs::{EventProofs, WitnessedEvent},
};
use crate::configs::FullClient;
use async_trait::async_trait;
use futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use node_runtime::{
    self,
    opaque::{Block, BlockId},
    pallet_validated_streams::ExtrinsicDetails,
};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use sc_transaction_pool::{BasicPool, FullChainApi};
use sc_transaction_pool_api::TransactionSource;
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::AuraApi;
use sp_core::{
    sr25519::{Public, Signature},
    ByteArray, H256,
};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, BoundedBTreeMap, BoundedVec};
#[cfg(test)]
pub mod tests;
use sp_runtime::{app_crypto::RuntimePublic, key_types::AURA, OpaqueExtrinsic};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
pub use tonic::{transport::Server, Request, Response, Status};
const TX_SOURCE: TransactionSource = TransactionSource::Local;

/// Internal struct holding the latest block state in the EventService
#[derive(Clone, Debug)]
struct EventServiceBlockState {
	pub validators: Vec<CryptoTypePublicPair>,
}
impl EventServiceBlockState {
	/// creates a new EventServiceBlockState
	pub fn new(validators: Vec<CryptoTypePublicPair>) -> Self {
		Self { validators }
	}

	/// verifies whether the received witnessed event was originited by one of the validators
	/// than proceeds to retrieving the pubkey and the signature and checks the signature
	pub fn verify_witnessed_event_origin(
		&self,
		witnessed_event: WitnessedEvent,
	) -> Result<WitnessedEvent, Error> {
		if self.validators.contains(&witnessed_event.pub_key) {
			let pubkey =
				Public::from_slice(witnessed_event.pub_key.1.as_slice()).map_err(|_| {
					Error::Other("can't retrieve sr25519 keys from WitnessedEvent".to_string())
				})?;
			let signature = Signature::from_slice(witnessed_event.signature.as_slice())
				.ok_or_else(|| {
					Error::Other("can't create sr25519 signature from witnessed event".to_string())
				})?;
			if pubkey.verify(&witnessed_event.event_id, &signature) {
				Ok(witnessed_event)
			} else {
				Err(Error::Other("incorrect gossip message signature".to_string()))
			}
		} else {
			Err(Error::Other("received a gossip message from a non validator".to_string()))
		}
	}

	/// calcultes the minimum number of validators to witness an event in order for it to be valid
	pub fn target(&self) -> u16 {
		(2 * ((self.validators.len() - 1) / 3) + 1) as u16
	}
}

/// A service which handles incoming events from the trusted client and other nodes.
/// It maintains the proofs that enter [EventProofs] storage, handles incoming gossip,
/// and submits extrinsics for proofs that we have collected the necessary signatures for.
pub struct EventService {
	block_state: Arc<RwLock<EventServiceBlockState>>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	streams_gossip: StreamsGossip,
	keystore: Arc<dyn CryptoStore>,
	tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	client: Arc<FullClient>,
}
impl EventService {
	/// Creates a new EventService
	pub async fn new(
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		streams_gossip: StreamsGossip,
		keystore: Arc<dyn CryptoStore>,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
		client: Arc<FullClient>,
	) -> EventService {
		let block_state = Arc::new(RwLock::new(EventServiceBlockState::new(vec![])));
		Self::handle_imported_blocks(client.clone(), block_state.clone()).await;
		EventService { block_state, event_proofs, streams_gossip, keystore, tx_pool, client }
	}

	fn witnessed_events_topic() -> IdentTopic {
		IdentTopic::new("WitnessedEvent")
	}

	/// receives client requests for handling incoming witnessed events, if the event has not been
	/// witnessed previously it adds it to the EventProofs and gossips the event for other
	/// validators
	pub async fn handle_client_request(&self, event: H256) -> Result<String, Error> {
		let witnessed_event = self.sign_witnessed_event(event).await?;
		let response = self.handle_witnessed_event(witnessed_event.clone()).await?;
		let serilized_event = bincode::serialize(&witnessed_event)
			.map_err(|e| Error::SerilizationFailure(e.to_string()))?;
		self.streams_gossip
			.clone()
			.publish(Self::witnessed_events_topic(), serilized_event)
			.await;
		Ok(response)
	}

	/// creates a signed witnessed event messages
	async fn sign_witnessed_event(&self, event_id: H256) -> Result<WitnessedEvent, Error> {
		let block_state = self.block_state.read()?.clone();

		let supported_keys = self
			.keystore
			.supported_keys(AURA, block_state.validators)
			.await
			.map_err(|e| Error::Other(e.to_string()))?;
		//log::info!("node is currently a validator");

		let signing_pubkey = supported_keys
			.get(0)
			.ok_or_else(|| Error::Other("Not a validator".to_string()))?;

		if let Some(sig) = self
			.keystore
			.sign_with(AURA, signing_pubkey, event_id.as_bytes())
			.await
			.map_err(|e| Error::Other(e.to_string()))?
		{
			Ok(WitnessedEvent { signature: sig, pub_key: signing_pubkey.clone(), event_id })
		} else {
			Err(Error::Other("Failed retrieving signature".to_string()))
		}
	}

	/// calculates the target from the latest finalized block and checks whether each event in ids
	/// reaches the target, it returns a result that contains only the events that did Not reach
	/// the target yet or completely unwitnessed events
	pub fn verify_events_validity(
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		ids: Vec<H256>,
	) -> Result<Vec<H256>, Error> {
		let block_state =
			Self::get_block_state(client.clone(), BlockId::hash(client.info().finalized_hash))?;
		let target = block_state.target();
		event_proofs.purge_stale_signatures(&block_state.validators, &ids)?;
		let mut unprepared_ids = Vec::new();
		for id in ids {
			let current_count = event_proofs.get_proof_count(id)?;
			if current_count < target {
				unprepared_ids.push(id);
			}
		}
		Ok(unprepared_ids)
	}

	/// starts a loop in another thread that listens for incoming finalized block and update the
	/// list of validators after each one
	async fn handle_imported_blocks(
		client: Arc<FullClient>,
		block_state: Arc<RwLock<EventServiceBlockState>>,
	) {
		tokio::spawn(async move {
			loop {
				let finality_notification =
					client.finality_notification_stream().select_next_some().await;

				if let Err(e) =
					Self::get_block_state(client.clone(), BlockId::hash(finality_notification.hash))
						.map(|public_keys| {
							block_state.write().map(|mut guard| *guard = public_keys.clone())
						}) {
					log::error!("{}", e.to_string());
				}
			}
		});
	}

	/// updates the list of validators
	fn get_block_state(
		client: Arc<FullClient>,
		block_id: BlockId,
	) -> Result<EventServiceBlockState, Error> {
		let public_keys = client
			.runtime_api()
			.authorities(&block_id)
			.map_err(|e| Error::Other(e.to_string()))?
			.iter()
			.map(CryptoTypePublicPair::from)
			.collect();

		Ok(EventServiceBlockState::new(public_keys))
	}
}

/// Allows EventService to be used as a handler for StreamsGossip
#[async_trait]
impl StreamsGossipHandler for EventService {
	fn get_topics() -> Vec<IdentTopic> {
		vec![Self::witnessed_events_topic()]
	}

	async fn handle(&self, message_data: Vec<u8>) {
		match bincode::deserialize::<WitnessedEvent>(message_data.as_slice()) {
			Ok(witnessed_event) => {
				self.handle_witnessed_event(witnessed_event).await.ok();
			},
			Err(e) => log::error!("failed deserilizing message data due to error:{:?}", e),
		}
	}
}
impl EventService {
	/// every incoming WitnessedEvent event should go through this function for processing the
	/// message outcome, it verifies the WitnessedEvent than it tries to add it to the EventProofs,
	/// and if its not already added it checks whether it reached the required target or not, if it
	/// did it submits it to the transaction pool
	async fn handle_witnessed_event(
		&self,
		witnessed_event: WitnessedEvent,
	) -> Result<String, Error> {
		let (witnessed_event, target) = {
			let block_state = &self.block_state.read()?;
			(block_state.verify_witnessed_event_origin(witnessed_event)?, block_state.target())
		};

		match self.event_proofs.add_event_proof(&witnessed_event) {
			Ok(proof_count) =>
				if proof_count == target {
					// avoid purging stale signatures everytime an event gets added, just check it
					// only when proof count is updated
					let proof_count = self.event_proofs.purge_stale_signature(
						&self.block_state.read()?.validators,
						witnessed_event.event_id,
					)?;
					if proof_count >= target {
						#[cfg(not(feature = "on-chain-proofs"))]
						self.submit_event_extrinsic(witnessed_event.event_id, None).await?;
						#[cfg(feature = "on-chain-proofs")]
						self.submit_event_extrinsic(
							witnessed_event.event_id,
							Some(self.event_proofs.get_event_proofs(&witnessed_event.event_id)?),
						)
						.await?;
						Ok(format!("Event:{} has been witnessed by a mjority of validators and is in TXPool, Current Proof count:{}",witnessed_event.event_id,proof_count))
					} else {
						Ok(format!(
							"Event:{} has been added to the event proofs, Current Proof Count:{}",
							witnessed_event.event_id, proof_count
						))
					}
				} else {
					Ok(format!(
						"Event:{} has been added to the event proofs, Current Proof Count:{}",
						witnessed_event.event_id, proof_count
					))
				},
			Err(e) => {
				log::info!("{}", e);
				Err(e)
			},
		}
	}
	/// create a validated streams unsigned extrinsic with the given event_id and submits it to the
	/// transaction pool
	async fn submit_event_extrinsic(
		&self,
		event_id: H256,
		event_proofs: Option<HashMap<CryptoTypePublicPair, Vec<u8>>>,
	) -> Result<H256, Error> {
		let proofs = {
			if let Some(mut event_proofs) = event_proofs {
				let proofs =
					event_proofs.iter_mut().fold(BoundedBTreeMap::new(), |mut proofs, (k, v)| {
						let pubkey = Public::from_slice(k.1.as_slice()).unwrap();
						let signature: BoundedVec<_, _> = v.clone().try_into().unwrap();
						proofs.try_insert(pubkey, signature).unwrap();
						proofs
					});
				Some(proofs)
			} else {
				None
			}
		};
		let best_block_id = BlockId::hash(self.client.info().best_hash);
		let unsigned_extrinsic = self
			.client
			.runtime_api()
			.create_unsigned_extrinsic(&best_block_id, event_id, proofs)
			.map_err(|e| Error::Other(e.to_string()))?;
		let opaque_extrinsic = OpaqueExtrinsic::from(unsigned_extrinsic);
		self.tx_pool
			.pool()
			.submit_one(&best_block_id, TX_SOURCE, opaque_extrinsic)
			.await
			.map_err(|e| Error::Other(e.to_string()))
	}
}
