//! Service which witnesses events from the trusted client

use crate::{errors::Error, gossip::Gossip, proofs::WitnessedEvent, traits::EventWitnesserTrait};
use async_trait::async_trait;
use codec::Codec;
use libp2p::gossipsub::IdentTopic;
use pallet_validated_streams::ValidatedStreamsApi;
use sc_client_api::HeaderBackend;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::H256;
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, key_types::AURA};
use std::{marker::PhantomData, sync::Arc};

use super::{get_latest_authorities_list, gossip::WITNESSED_EVENTS_TOPIC};

/// A service which signs and submits proofs for events we have witnessed.
pub struct EventWitnesser<Block, Client, AuthorityId> {
	client: Arc<Client>,
	gossip: Gossip,
	keystore: Arc<dyn CryptoStore>,
	phantom: PhantomData<(Block, AuthorityId)>,
}

impl<Block, Client, AuthorityId> EventWitnesser<Block, Client, AuthorityId> {
	/// Creates a new [EventWitnesser] with the given keystore and gossip
	pub fn new(client: Arc<Client>, gossip: Gossip, keystore: Arc<dyn CryptoStore>) -> Self {
		Self { client, gossip, keystore, phantom: PhantomData }
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
		let block_state = get_latest_authorities_list(self.client.as_ref())?;

		let supported_keys = self.keystore.supported_keys(AURA, block_state.authorities).await?;

		let pub_key = supported_keys.get(0).ok_or(Error::NotAValidator)?;

		let signature = self
			.keystore
			.sign_with(AURA, pub_key, event_id.as_bytes())
			.await?
			.ok_or_else(|| Error::SigningFailure("Failed getting a signature".to_string()))?;

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
