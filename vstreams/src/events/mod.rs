//! Service which handles incoming events from the trusted client and other nodes

use crate::{
	errors::Error,
	proofs::{EventProofsTrait, WitnessedEvent},
};
use codec::Codec;
use lru::LruCache;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::HeaderBackend;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::app_crypto::{CryptoTypePublicPair, RuntimePublic};
use std::sync::{Arc, Mutex};

#[cfg(test)]
pub mod tests;

pub mod gossip;
pub use gossip::EventGossipHandler;

pub mod witness;
pub use witness::EventWitnesser;

pub mod validate;
pub use validate::EventValidator;

/// Internal struct holding the latest block state in the EventService
#[derive(Clone, Debug)]
pub struct EventServiceBlockState {
	/// list of validators represented by their public keys
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

/// calculates the target from the latest finalized block and checks whether each event in ids
/// reaches the target, it returns a result that contains only the events that did Not reach
/// the target yet or completely unwitnessed events
pub fn verify_events_validity<Block, EventProofs, Client, AuthorityId>(
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>,
	client: Arc<Client>,
	authorities_block_id: <Block as BlockT>::Hash,
	event_proofs: Arc<EventProofs>,
	ids: Vec<H256>,
) -> Result<Vec<H256>, Error>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	EventProofs: EventProofsTrait + Send + Sync,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	let block_state = get_block_state(block_state,client.as_ref(), authorities_block_id)?;
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
fn get_latest_block_state<Block, Client, AuthorityId>(
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>,
	client: &Client,
) -> Result<EventServiceBlockState, Error>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	get_block_state(block_state,client, client.info().finalized_hash)
}

/// updates the list of validators
fn get_block_state<Block, Client, AuthorityId>(
	block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>,
	client: &Client,
	authorities_block_id: <Block as BlockT>::Hash,
	) -> Result<EventServiceBlockState, Error>
where
Block: BlockT,
Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
AuthorityId: Codec + Send + Sync + 'static,
CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	if let Some(block_state) = block_state.lock().or(Err(Error::LockFail("BlockState".to_string())))?.get(&authorities_block_id) {
		println!("HIT");
        return Ok(block_state.clone());
	}
	println!("MISS");
	let public_keys = client
		.runtime_api()
		.authorities(authorities_block_id)
		.map_err(|e| Error::Other(e.to_string()))?
		.iter()
		.map(CryptoTypePublicPair::from)
		.collect();
	let new_block_state= EventServiceBlockState::new(public_keys);
	block_state.lock().or(Err(Error::LockFail("BlockState".to_string())))?.put(authorities_block_id,new_block_state.clone());

	Ok(new_block_state)
}
