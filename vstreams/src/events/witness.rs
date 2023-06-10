//! Service which handles incoming events from the trusted client and other nodes

use crate::{
	errors::Error, gossip::StreamsGossip, proofs::WitnessedEvent, traits::EventWitnesserTrait,
};
use async_trait::async_trait;
use codec::Codec;
use libp2p::gossipsub::IdentTopic;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::HeaderBackend;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::H256;
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, key_types::AURA};
use std::{marker::PhantomData, sync::Arc};

use super::{get_latest_block_state, gossip::WITNESSED_EVENTS_TOPIC};

/// A utility which signs and submits proofs for events we have witnessed.
pub struct EventWitnesser<Block, Client, AuthorityId> {
	client: Arc<Client>,
	streams_gossip: StreamsGossip,
	keystore: Arc<dyn CryptoStore>,
	phantom: PhantomData<(Block, AuthorityId)>,
}

impl<Block, Client, AuthorityId> EventWitnesser<Block, Client, AuthorityId> {
	/// Creates a new EventService
	pub fn new(
		client: Arc<Client>,
		streams_gossip: StreamsGossip,
		keystore: Arc<dyn CryptoStore>,
	) -> Self {
		Self { client, streams_gossip, keystore, phantom: PhantomData }
	}
}

#[async_trait]
impl<Block, Client, AuthorityId> EventWitnesserTrait for EventWitnesser<Block, Client, AuthorityId>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	/// receives client requests for handling incoming witnessed events, if the event has not been
	/// witnessed previously it adds it to the EventProofs and gossips the event for other
	/// validators
	async fn witness_event(&self, event_id: H256) -> Result<(), Error> {
		let block_state = get_latest_block_state(self.client.as_ref())?;

		let supported_keys = self
			.keystore
			.supported_keys(AURA, block_state.validators)
			.await
			.map_err(|e| Error::Other(e.to_string()))?;
		//log::info!("node is currently a validator");

		let pub_key = supported_keys
			.get(0)
			.ok_or_else(|| Error::Other("Not a validator".to_string()))?;

		let signature = self
			.keystore
			.sign_with(AURA, pub_key, event_id.as_bytes())
			.await
			.map_err(|e| Error::Other(e.to_string()))?
			.ok_or_else(|| Error::Other("Failed retrieving signature".to_string()))?;

		let witnessed_event = WitnessedEvent { signature, pub_key: pub_key.clone(), event_id };

		let serilized_event = bincode::serialize(&witnessed_event)
			.map_err(|e| Error::SerilizationFailure(e.to_string()))?;

		self.streams_gossip
			.clone()
			.publish(IdentTopic::new(WITNESSED_EVENTS_TOPIC), serilized_event)
			.await;

		Ok(())
	}
}
