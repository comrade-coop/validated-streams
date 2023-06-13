//! Service which proccesses all the incoming events

use crate::{
	errors::Error,
	gossip::StreamsGossipHandler,
	proofs::{EventProofsTrait, WitnessedEvent},
};
use async_trait::async_trait;
use codec::Codec;
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
use std::sync::Mutex;
use super::{get_latest_block_state, EventServiceBlockState};
use sp_runtime::{app_crypto::CryptoTypePublicPair, generic::BlockId};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};
extern crate lru;

use lru::LruCache;
/// The topic on which the [EventGossipHandler] listens.
pub const WITNESSED_EVENTS_TOPIC: &str = "WitnessedEvent";

/// Service that handles incoming gossip, maintains the [EventProofs] storage,
/// and submits extrinsics for proofs that we have collected the necessary signatures for.
pub struct EventGossipHandler<TxPool, Client, EventProofs, AuthorityId, Block:BlockT> {
	event_proofs: Arc<EventProofs>,
	tx_pool: Arc<TxPool>,
	client: Arc<Client>,
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>,
	phantom: PhantomData<AuthorityId>,
}

impl<TxPool, Client, EventProofs, AuthorityId, Block>
	EventGossipHandler<TxPool, Client, EventProofs, AuthorityId,Block>
where
	TxPool: LocalTransactionPool + LocalTransactionPool<Block = Block>,
	Client: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
	EventProofs: EventProofsTrait + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	Block:BlockT,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<TxPool::Block> + AuraApi<TxPool::Block, AuthorityId>,
{
	/// Creates a new EventGossipHandler
	pub fn new(block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>, client: Arc<Client>, event_proofs: Arc<EventProofs>, tx_pool: Arc<TxPool>) -> Self {

		Self { client, event_proofs, tx_pool, phantom: PhantomData, block_state}
	}

	/// every incoming WitnessedEvent event should go through this function for processing the
	/// message outcome, it verifies the WitnessedEvent than it tries to add it to the EventProofs,
	/// and if its not already added it checks whether it reached the required target or not, if it
	/// did it submits it to the transaction pool
	async fn handle_witnessed_event(&self, witnessed_event: WitnessedEvent) -> Result<bool, Error> {
		let block_state = get_latest_block_state(self.block_state.clone(),self.client.as_ref())?;
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

			self.submit_event_extrinsic(witnessed_event.event_id, proofs).await?;
		} else {
			log::debug!(
				"Event:{} has been added to the event proofs, Current Proof Count:{}",
				witnessed_event.event_id,
				proof_count
			);
		}

		Ok(true)
	}

	/// create a validated streams unsigned extrinsic with the given event_id and submits it to the
	/// transaction pool
	async fn submit_event_extrinsic(
		&self,
		event_id: H256,
		event_proofs: Option<HashMap<CryptoTypePublicPair, Vec<u8>>>,
	) -> Result<(), Error> {
		let proofs = {
			if let Some(event_proofs) = event_proofs {
				let proofs = event_proofs
					.iter()
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
		let best_hash = self.client.info().best_hash;
		let unsigned_extrinsic = self
			.client
			.runtime_api()
			.create_unsigned_extrinsic(best_hash, event_id, proofs)
			.map_err(|e| Error::Other(e.to_string()))?;

		match self.tx_pool.submit_local(&BlockId::hash(best_hash), unsigned_extrinsic) {
			Ok(_) => Ok(()),
			Err(x) => match x.into_pool_error() {
				Ok(PoolError::AlreadyImported(_)) => Ok(()),
				Ok(e) => Err(Error::Other(e.to_string())),
				Err(e) => Err(Error::Other(e.to_string())),
			},
		}
	}
}

#[async_trait]
impl<TxPool, Client, EventProofs, AuthorityId, Block> StreamsGossipHandler
	for EventGossipHandler<TxPool, Client, EventProofs, AuthorityId, Block>
where
	TxPool: LocalTransactionPool + LocalTransactionPool<Block = Block>,
	Client: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ Send
		+ Sync
		+ 'static,
	EventProofs: EventProofsTrait + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Block: BlockT,
	Client::Api: ExtrinsicDetails<TxPool::Block> + AuraApi<TxPool::Block, AuthorityId>,
{
	fn get_topics() -> Vec<IdentTopic> {
		vec![IdentTopic::new(WITNESSED_EVENTS_TOPIC)]
	}

	async fn handle(&self, message_data: Vec<u8>) {
		match bincode::deserialize::<WitnessedEvent>(message_data.as_slice()) {
			Ok(witnessed_event) => {
				if let Err(e) = self.handle_witnessed_event(witnessed_event).await {
					log::error!("failed processing message: {:?}", e)
				}
			},
			Err(e) => log::error!("failed deserilizing message data due to error:{:?}", e),
		}
	}
}
