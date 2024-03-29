//! Service which witnesses events from the trusted client

use super::{get_latest_authorities_list, gossip::WITNESSED_EVENTS_TOPIC, AuthoritiesList};
use crate::{errors::Error, gossip::Gossip, proofs::WitnessedEvent, traits::EventWitnesserTrait};
use async_trait::async_trait;
use codec::Codec;
use libp2p::gossipsub::IdentTopic;
use lru::LruCache;
use pallet_validated_streams::ValidatedStreamsApi;
use sc_client_api::HeaderBackend;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::H256;
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, key_types::AURA};
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
};

/// A utility which signs and submits proofs for events we have witnessed.
pub struct EventWitnesser<Block: BlockT, Client, AuthorityId> {
	client: Arc<Client>,
	gossip: Gossip,
	keystore: Arc<dyn CryptoStore>,
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash, AuthoritiesList>>>,
	phantom: PhantomData<(Block, AuthorityId)>,
}

impl<Block, Client, AuthorityId> EventWitnesser<Block, Client, AuthorityId>
where
	Block: BlockT,
{
	/// Creates a new EventService
	pub fn new(
		client: Arc<Client>,
		gossip: Gossip,
		keystore: Arc<dyn CryptoStore>,
		block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash, AuthoritiesList>>>,
	) -> Self {
		Self { client, gossip, keystore, phantom: PhantomData, block_state }
	}
}

#[async_trait]
impl<Block, Client, AuthorityId> EventWitnesserTrait for EventWitnesser<Block, Client, AuthorityId>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
{
	/// Witnesses an event by signing and sending it to the [Gossip].
	/// [EventGossipHandler] will then proceed to add the event to the [EventProofsTrait].
	async fn witness_event(&self, event_id: H256) -> Result<(), Error> {
		let block_state =
			get_latest_authorities_list(self.block_state.clone(), self.client.as_ref())?;

		log::trace!("To witness event {event_id} {}", event_id);

		let supported_keys = self.keystore.supported_keys(AURA, block_state.authorities).await?;

		let pub_key = supported_keys.get(0).ok_or(Error::NotAValidator)?;
		let signature = self
			.keystore
			.sign_with(AURA, pub_key, event_id.as_bytes())
			.await?
			.ok_or_else(|| Error::SigningFailure("Failed getting a signature".to_string()))?;

		log::trace!("Signed event {event_id} {}", event_id);

		let witnessed_event = WitnessedEvent { signature, pub_key: pub_key.clone(), event_id };

		let serilized_event = bincode::serialize(&witnessed_event)
			.map_err(|e| Error::SerilizationFailure(e.to_string()))?;

		self.gossip
			.clone()
			.publish(IdentTopic::new(WITNESSED_EVENTS_TOPIC), serilized_event)
			.await;

		Ok(())
	}
}
