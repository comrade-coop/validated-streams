//! Service which handles incoming events from the trusted client and other nodes

use crate::{
	errors::Error,
	proofs::{EventProofsTrait, WitnessedEvent},
};
use codec::Codec;
use lru::LruCache;
use pallet_validated_streams::ValidatedStreamsApi;
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

mod gossip;
mod validate;
mod witness;

pub use gossip::EventGossipHandler;
pub use validate::EventValidator;
pub use witness::EventWitnesser;

/// A cache for the list of authorities in a block.
pub type BlockStateCache<Block> = Arc<Mutex<LruCache<<Block as BlockT>::Hash, AuthoritiesList>>>;

/// Internal struct holding the list of the authorities at a particular block.
#[derive(Clone, Debug)]
pub struct AuthoritiesList {
	/// The list of authorities at the block.
	pub authorities: Vec<CryptoTypePublicPair>,
}
impl AuthoritiesList {
	/// Creates a new [AuthoritiesList]
	pub fn new(authorities: Vec<CryptoTypePublicPair>) -> Self {
		Self { authorities }
	}

	/// Verifies that the witnessed event was signed by one of the authorities
	/// than proceeds to check the signature
	pub fn verify_witnessed_event_origin(
		&self,
		witnessed_event: WitnessedEvent,
	) -> Result<WitnessedEvent, Error> {
		if self.authorities.contains(&witnessed_event.pub_key) {
			let pubkey =
				Public::from_slice(witnessed_event.pub_key.1.as_slice()).map_err(|_| {
					Error::BadWitnessedEventSignature(
						"Can't retrieve sr25519 keys from WitnessedEvent".to_string(),
					)
				})?;
			let signature = Signature::from_slice(witnessed_event.signature.as_slice())
				.ok_or_else(|| {
					Error::BadWitnessedEventSignature(
						"Can't create sr25519 signature from witnessed event".to_string(),
					)
				})?;

			if pubkey.verify(&witnessed_event.event_id, &signature) {
				Ok(witnessed_event)
			} else {
				Err(Error::BadWitnessedEventSignature(
					"Incorrect WitnessedEvent signature".to_string(),
				))
			}
		} else {
			Err(Error::BadWitnessedEventSignature(
				"WitnessedEvent was signed by non-validator".to_string(),
			))
		}
	}

	/// Calcultes the minimum number of authorities to witness an event in order for it to be valid.
	/// --
	/// Currently, this uses the formula floor(n * 2 / 3) + 1; the logic for that is slightly
	/// convoluted but in short, GRANDPA tolerates `f` Byzantine failures as long as `f < 3n`, and
	/// sticking with that same amount of tolerated failures, we want to know the minimum amount of
	/// nodes to witness an event so that a majority of nodes can be considered to have witnessed
	/// it. Conceptually, if every non-failing node votes for event A or event B, but not both, we
	/// want the Validated Streams network to finalize A, B, or neither, but not both. Since the
	/// up-to-`f` Byzantine nodes can vote for both A and B, the lowest amount of votes past
	/// which A (or B) can be considered final is the number needed for a strict majority of the non
	/// failing nodes plus the number of double-voting nodes -- or (n - n//3)//2 + 1 + n//3, which
	/// just so happens to equal n * 2 // 3 after rounding.
	pub fn target(&self) -> u16 {
		let total = self.authorities.len();
		(total * 2 / 3 + 1) as u16
	}
}

/// Returns the list of events that we do not have enough witnesses for, using the authorities in
/// the given block.
pub(crate) fn verify_events_validity<Block, EventProofs, Client, AuthorityId>(
	block_state: BlockStateCache<Block>,
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
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
{
	let authorities_list =
		get_authorities_list(block_state, client.as_ref(), authorities_block_id)?;
	let target = authorities_list.target();
	let mut unprepared_ids = Vec::new();
	for id in ids {
		let current_count =
			event_proofs.get_event_proof_count(&id, &authorities_list.authorities)?;
		if current_count < target {
			unprepared_ids.push(id);
		}
	}
	Ok(unprepared_ids)
}

/// Reads the latest finalized list of authorities. For use when pruining event proofs.
pub(crate) fn get_latest_authorities_list<Block, Client, AuthorityId>(
	block_state: BlockStateCache<Block>,
	client: &Client,
) -> Result<AuthoritiesList, Error>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
{
	get_authorities_list(block_state, client, client.info().finalized_hash)
}

/// Reads the list of authorities from a block.
pub(crate) fn get_authorities_list<Block, Client, AuthorityId>(
	block_state: BlockStateCache<Block>,
	client: &Client,
	authorities_block_id: <Block as BlockT>::Hash,
) -> Result<AuthoritiesList, Error>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
{
	if let Some(block_state) = block_state.lock()?.get(&authorities_block_id) {
		return Ok(block_state.clone())
	}
	let public_keys = client
		.runtime_api()
		.authorities(authorities_block_id)
		.map_err(|e| Error::Other(e.to_string()))?
		.iter()
		.map(CryptoTypePublicPair::from)
		.collect();
	let new_block_state = AuthoritiesList::new(public_keys);
	block_state.lock()?.put(authorities_block_id, new_block_state.clone());

	Ok(new_block_state)
}
