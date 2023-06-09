//! Service which handles incoming events from the trusted client and other nodes

use crate::{
	errors::Error,
	gossip::{StreamsGossip, StreamsGossipHandler},
	proofs::{EventProofs, WitnessedEvent},
};
use async_trait::async_trait;
use codec::Codec;
use futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockchainEvents, HeaderBackend};
use sc_transaction_pool_api::{
	error::{Error as PoolError, IntoPoolError},
	LocalTransactionPool,
};
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_keystore::CryptoStore;
use sp_runtime::{
	app_crypto::{CryptoTypePublicPair, RuntimePublic},
	generic::BlockId,
	key_types::AURA,
};
use std::{
	collections::HashMap,
	marker::PhantomData,
	sync::{Arc, RwLock},
};
#[cfg(test)]
pub mod tests;

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
					Error::BadWitnessedEventSignature(
						"can't retrieve sr25519 keys from WitnessedEvent".to_string(),
					)
				})?;
			let signature = Signature::from_slice(witnessed_event.signature.as_slice())
				.ok_or_else(|| {
					Error::BadWitnessedEventSignature(
						"can't create sr25519 signature from witnessed event".to_string(),
					)
				})?;

			if pubkey.verify(&witnessed_event.event_id, &signature) {
				Ok(witnessed_event)
			} else {
				Err(Error::BadWitnessedEventSignature(
					"incorrect gossip message signature".to_string(),
				))
			}
		} else {
			Err(Error::BadWitnessedEventSignature(
				"received a gossip message from a non validator".to_string(),
			))
		}
	}

	/// calcultes the minimum number of validators to witness an event in order for it to be valid
	pub fn target(&self) -> u16 {
		let total = self.validators.len();
		(total - total / 3) as u16
	}
}

/// A trait wrapping the [EventService]'s functionality of witnessing an event, called by the
/// trusted client (e.g. through GRPC).
#[async_trait]
pub trait EventWitnessHandler {
	/// receives client requests for handling incoming witnessed events
	async fn witness_event(&self, event: H256) -> Result<(), Error>;
}

/// A service which handles incoming events from the trusted client and other nodes.
/// It maintains the proofs that enter [EventProofs] storage, handles incoming gossip,
/// and submits extrinsics for proofs that we have collected the necessary signatures for.
pub struct EventService<TxPool, Client, AuthorityId: Send + Sync> {
	block_state: Arc<RwLock<EventServiceBlockState>>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	streams_gossip: StreamsGossip,
	keystore: Arc<dyn CryptoStore>,
	tx_pool: Arc<TxPool>,
	client: Arc<Client>,
	phantom: PhantomData<AuthorityId>,
}

impl<
		TxPool: LocalTransactionPool,
		Client: ProvideRuntimeApi<TxPool::Block>
			+ BlockchainEvents<TxPool::Block>
			+ HeaderBackend<TxPool::Block>
			+ Send
			+ Sync
			+ 'static,
		AuthorityId: Codec + Send + Sync + 'static,
	> EventService<TxPool, Client, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<TxPool::Block> + AuraApi<TxPool::Block, AuthorityId>,
{
	/// Creates a new EventService
	pub async fn new(
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		streams_gossip: StreamsGossip,
		keystore: Arc<dyn CryptoStore>,
		tx_pool: Arc<TxPool>,
		client: Arc<Client>,
	) -> Self {
		let block_state = Arc::new(RwLock::new(EventServiceBlockState::new(vec![])));
		Self::handle_imported_blocks(client.clone(), block_state.clone()).await;
		Self {
			block_state,
			event_proofs,
			streams_gossip,
			keystore,
			tx_pool,
			client,
			phantom: PhantomData,
		}
	}

	fn witnessed_events_topic() -> IdentTopic {
		IdentTopic::new("WitnessedEvent")
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

	/// starts a loop in another thread that listens for incoming finalized block and update the
	/// list of validators after each one
	async fn handle_imported_blocks(
		client: Arc<Client>,
		block_state: Arc<RwLock<EventServiceBlockState>>,
	) {
		tokio::spawn(async move {
			loop {
				let finality_notification =
					client.finality_notification_stream().select_next_some().await;

				if let Err(e) =
					get_block_state(client.clone(), finality_notification.hash).map(|public_keys| {
						block_state.write().map(|mut guard| *guard = public_keys.clone())
					}) {
					log::error!("{}", e.to_string());
				}
			}
		});
	}

	/// every incoming WitnessedEvent event should go through this function for processing the
	/// message outcome, it verifies the WitnessedEvent than it tries to add it to the EventProofs,
	/// and if its not already added it checks whether it reached the required target or not, if it
	/// did it submits it to the transaction pool
	async fn handle_witnessed_event(&self, witnessed_event: WitnessedEvent) -> Result<bool, Error> {
		let (witnessed_event_to_submit, proofs) = {
			let block_state = self.block_state.read()?;
			let witnessed_event = block_state.verify_witnessed_event_origin(witnessed_event)?;

			self.event_proofs.add_event_proof(&witnessed_event)?;

			self.event_proofs
				.purge_event_stale_signatures(&witnessed_event.event_id, &block_state.validators)?;

			let proof_count = self
				.event_proofs
				.get_event_proof_count(&witnessed_event.event_id, &block_state.validators)?;

			if proof_count >= block_state.target() {
				#[cfg(not(feature = "on-chain-proofs"))]
				let proofs = None;
				#[cfg(feature = "on-chain-proofs")]
				let proofs = Some(
					self.event_proofs
						.get_event_proofs(&witnessed_event.event_id, &block_state.validators)?,
				);

				log::debug!(
					"Event:{} has been witnessed by a majority of validators and will be added to TxPool, Current Proof count:{}",
					witnessed_event.event_id,
					proof_count
				);

				(Some(witnessed_event), proofs)
			} else {
				log::debug!(
					"Event:{} has been added to the event proofs, Current Proof Count:{}",
					witnessed_event.event_id,
					proof_count
				);

				(None, None)
			}
		};

		if let Some(witnessed_event) = witnessed_event_to_submit {
			self.submit_event_extrinsic(witnessed_event.event_id, proofs).await?;
		}

		Ok(true)
		/*
			Err(e) => {
				log::info!("{}", e);
				Err(e)
			},
		*/
	}
	/// create a validated streams unsigned extrinsic with the given event_id and submits it to the
	/// transaction pool
	async fn submit_event_extrinsic(
		&self,
		event_id: H256,
		event_proofs: Option<HashMap<CryptoTypePublicPair, Vec<u8>>>,
	) -> Result<(), Error> {
		let proofs = {
			if let Some(mut event_proofs) = event_proofs {
				let proofs = event_proofs
					.iter_mut()
					.map(|(k, v)| {
						let pubkey = Public::from_slice(k.1.as_slice()).unwrap();
						let signature = Signature::from_slice(v.clone().as_slice()).unwrap();
						(pubkey, signature)
					})
					.collect();
				Some(proofs)
			} else {
				None
			}
		};
		let best_block_id = BlockId::hash(self.client.info().best_hash);
		let unsigned_extrinsic = self
			.client
			.runtime_api()
			.create_unsigned_extrinsic(self.client.info().best_hash, event_id, proofs)
			.map_err(|e| Error::Other(e.to_string()))?;

		match self.tx_pool.submit_local(&best_block_id, unsigned_extrinsic) {
			Ok(_) => Ok(()),
			Err(x) => match x.into_pool_error() {
				Ok(PoolError::AlreadyImported(_)) => Ok(()),
				Ok(e) => Err(Error::Other(e.to_string())),
				Err(e) => Err(Error::Other(e.to_string())),
			},
		}
	}
}

/// Allows EventService to be used as a handler for StreamsGossip
#[async_trait]
impl<
		TxPool: LocalTransactionPool,
		Client: ProvideRuntimeApi<TxPool::Block>
			+ HeaderBackend<TxPool::Block>
			+ BlockchainEvents<TxPool::Block>,
		AuthorityId: Codec + Send + Sync + 'static,
	> StreamsGossipHandler for EventService<TxPool, Client, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client: Send + Sync + 'static,
	Client::Api: ExtrinsicDetails<TxPool::Block> + AuraApi<TxPool::Block, AuthorityId>,
{
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

#[async_trait]
impl<
		TxPool: LocalTransactionPool,
		Client: ProvideRuntimeApi<TxPool::Block>
			+ HeaderBackend<TxPool::Block>
			+ BlockchainEvents<TxPool::Block>,
		AuthorityId: Codec + Send + Sync + 'static,
	> EventWitnessHandler for EventService<TxPool, Client, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client: Send + Sync + 'static,
	Client::Api: ExtrinsicDetails<TxPool::Block> + AuraApi<TxPool::Block, AuthorityId>,
{
	/// receives client requests for handling incoming witnessed events, if the event has not been
	/// witnessed previously it adds it to the EventProofs and gossips the event for other
	/// validators
	async fn witness_event(&self, event: H256) -> Result<(), Error> {
		let witnessed_event = self.sign_witnessed_event(event).await?;

		if self.handle_witnessed_event(witnessed_event.clone()).await? {
			let serilized_event = bincode::serialize(&witnessed_event)
				.map_err(|e| Error::SerilizationFailure(e.to_string()))?;
			self.streams_gossip
				.clone()
				.publish(Self::witnessed_events_topic(), serilized_event)
				.await;
		}

		Ok(())
	}
}

/// calculates the target from the latest finalized block and checks whether each event in ids
/// reaches the target, it returns a result that contains only the events that did Not reach
/// the target yet or completely unwitnessed events
pub fn verify_events_validity<
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
>(
	client: Arc<Client>,
	authorities_block_id: <Block as BlockT>::Hash,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	ids: Vec<H256>,
) -> Result<Vec<H256>, Error>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	let block_state = get_block_state(client, authorities_block_id)?;
	let target = block_state.target();
	let mut unprepared_ids = Vec::new();
	for id in ids {
		let current_count = event_proofs.get_event_proof_count(&id, &block_state.validators)?;
		if current_count < target {
			unprepared_ids.push(id);
		}
	}
	Ok(unprepared_ids)
}

/// updates the list of validators
fn get_block_state<
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
>(
	client: Arc<Client>,
	authorities_block_id: <Block as BlockT>::Hash,
) -> Result<EventServiceBlockState, Error>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	let public_keys = client
		.runtime_api()
		.authorities(authorities_block_id)
		.map_err(|e| Error::Other(e.to_string()))?
		.iter()
		.map(CryptoTypePublicPair::from)
		.collect();

	Ok(EventServiceBlockState::new(public_keys))
}