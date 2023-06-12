//! Service which handles incoming events from the trusted client and other nodes

use crate::{
	errors::Error,
	proofs::{EventProofsTrait, WitnessedEvent},
};
use codec::Codec;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::HeaderBackend;
use sp_api::{BlockT, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::app_crypto::{CryptoTypePublicPair, RuntimePublic};
use std::sync::Arc;

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

	/// Calcultes the minimum number of validators to witness an event in order for it to be valid.
	/// --
	/// Currently, this uses the formula floor(2/3 * n) + 1; the logic for that is slightly
	/// convoluted but in short, GRANDPA tolerates `f` Byzantine failures as long as `f < 3n`, and
	/// sticking with that same amount of tolerated failures, we want to know the minimum amount of
	/// nodes to witness an event so that a majority of nodes can be considered to have witnessed
	/// it. Conceptually, if every non-failing node votes for event A or event B, but not both, we
	/// want the Validated Streams network to finalize A or B or neither, but not both. Since the
	/// up-to-`f` Byzantine nodes can do vote for both A and B, the lowest amount of votes past
	/// which A (or B) can be considered final is the number needed for a strict majority of the non
	/// failing nodes + the number of double-voting nodes -- or (n - n//3)//2 + 1 + n//3, which just
	/// so happens to equal n * 2 // 3 despite the rounding.
	pub fn target(&self) -> u16 {
		let total = self.validators.len();
		(total * 2 / 3 + 1) as u16
	}
}

/// calculates the target from the latest finalized block and checks whether each event in ids
/// reaches the target, it returns a result that contains only the events that did Not reach
/// the target yet or completely unwitnessed events
pub fn verify_events_validity<Block, EventProofs, Client, AuthorityId>(
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
	let block_state = get_block_state(client.as_ref(), authorities_block_id)?;
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
	client: &Client,
) -> Result<EventServiceBlockState, Error>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	get_block_state(client, client.info().finalized_hash)
}

/// updates the list of validators
fn get_block_state<Block, Client, AuthorityId>(
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
	let public_keys = client
		.runtime_api()
		.authorities(authorities_block_id)
		.map_err(|e| Error::Other(e.to_string()))?
		.iter()
		.map(CryptoTypePublicPair::from)
		.collect();

	Ok(EventServiceBlockState::new(public_keys))
}
