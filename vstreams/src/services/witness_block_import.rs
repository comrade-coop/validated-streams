//! Block import which waits for all events to be witnessed before finalizing a block.
#![allow(unused_imports)]
use crate::{
	errors::Error,
	proofs::{EventProofs, ProofsMap},
	services::events::verify_events_validity,
};
use codec::{Codec, Decode, Encode};
use futures::{channel::oneshot, future::Shared, FutureExt, StreamExt};
use pallet_validated_streams::ExtrinsicDetails;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
pub use sc_executor::NativeElseWasmExecutor;
use sc_network::{
	DhtEvent, Event, KademliaKey, NetworkDHTProvider, NetworkEventStream, NetworkService,
};
use sp_api::{HeaderT, ProvideRuntimeApi};
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::{
	app_crypto::{CryptoTypePublicPair, RuntimePublic},
	generic::BlockId,
	traits::{Block as BlockT, BlockIdTo},
};
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, sync::Arc};
use tokio::sync::Mutex;
/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
pub struct WitnessBlockImport<Block: BlockT, I, Client, AuthorityId> {
	parent_block_import: I,
	#[cfg(not(feature = "on-chain-proofs"))]
	pub block_manager: Arc<BlockManager<Block, Client, AuthorityId>>,
}

// Derive clone doesn't work because Client and AuthorityId are not necessarily Clone.
// See
impl<Block: BlockT, I: Clone, Client, AuthorityId> Clone
	for WitnessBlockImport<Block, I, Client, AuthorityId>
{
	fn clone(&self) -> Self {
		Self {
			parent_block_import: self.parent_block_import.clone(),
			block_manager: self.block_manager.clone(),
		}
	}
}

/// conatiner and manager of deferred blocks
#[cfg(not(feature = "on-chain-proofs"))]
pub struct BlockManager<Block: BlockT, Client, AuthorityId> {
	/// provides access to the distributed hash table to the witness block import
	network_service: Shared<oneshot::Receiver<Arc<NetworkService<Block, H256>>>>,
	/// list of deferred blocks and their corresponding unwitnessed events
	deferred_blocks: Arc<Mutex<HashMap<<Block as BlockT>::Hash, Vec<H256>>>>,
	client: Arc<Client>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,

	#[allow(clippy::type_complexity)] // This rust type is painful...
	network_service_sender: Arc<Mutex<Option<oneshot::Sender<Arc<NetworkService<Block, H256>>>>>>,

	phantom: std::marker::PhantomData<AuthorityId>,
}

#[cfg(not(feature = "on-chain-proofs"))]
impl<Block: BlockT, Client, AuthorityId> BlockManager<Block, Client, AuthorityId> {
	pub fn new(client: Arc<Client>, event_proofs: Arc<dyn EventProofs + Send + Sync>) -> Self {
		let (network_service_sender, network_service) = oneshot::channel();
		Self {
			deferred_blocks: Arc::new(Mutex::new(HashMap::new())),
			network_service: network_service.shared(),
			network_service_sender: Arc::new(Mutex::new(Some(network_service_sender))),
			client,
			event_proofs,
			phantom: PhantomData,
		}
	}
}

#[cfg(not(feature = "on-chain-proofs"))]
impl<
		Block: BlockT,
		Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
		AuthorityId: Codec + Send + Sync + 'static,
	> BlockManager<Block, Client, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	/// handles incoming dht events and set the network service
	/// for all instances of the witness block import
	pub async fn handle_dht_events(
		self: Arc<Self>,
		network_service: Arc<NetworkService<Block, H256>>,
	) {
		if let Some(network_service_sender) =
			std::mem::replace(&mut *self.network_service_sender.lock().await, None)
		{
			network_service_sender.send(network_service.clone()).unwrap_err();
		}

		let deferred_blocks = self.deferred_blocks.clone();
		tokio::spawn(async move {
			while let Some(event) = network_service.event_stream("event_proofs").next().await {
				if let Event::Dht(e) = event {
					match e {
						DhtEvent::ValueFound(values) => self.handle_found_proofs(values).await,
						DhtEvent::ValueNotFound(key) => {
							log::info!("block key not found in dht");
							let Ok(deserialized_key) = <Block as BlockT>::Hash::decode(&mut key.to_vec().as_slice()) else {continue;};
							deferred_blocks.lock().await.remove(&deserialized_key);
						},
						DhtEvent::ValuePutFailed(_key) => {
							// log::info!("value put failed for {:?}",key);
						},
						DhtEvent::ValuePut(key) => {
							log::info!("key {:?} inserted in dht", key);
						},
					}
				}
			}
		});
	}
	async fn handle_found_proofs(&self, values: Vec<(KademliaKey, Vec<u8>)>) {
		for value in values {
			let mut deferred_blocks = self.deferred_blocks.lock().await;
			let (kademlia_key, value) = value;
			let key_vec = kademlia_key.to_vec();
			if let Ok(key) = <Block as BlockT>::Hash::decode(&mut key_vec.as_slice()) {
				if deferred_blocks.contains_key(&key) {
					if let Ok(proofs) = bincode::deserialize::<ProofsMap>(&value) {
						let unwitnessed_events = deferred_blocks.get(&key).unwrap();
						if let Ok(result) = Self::verify_proofs(
							key,
							&proofs,
							unwitnessed_events,
							self.client.clone(),
						) {
							if result {
								log::info!("💡 Retreived all event proofs of block {}", key);
								self.event_proofs.add_events_proofs(proofs).ok();
								deferred_blocks.remove(&key);
							}
						} else {
							deferred_blocks.remove(&key);
						}
					} else {
						log::error!("failed deserializing proofs");
					}
				}
			} else {
				log::error!("bad block key");
			}
		}
	}
	fn verify_proofs(
		block: <Block as BlockT>::Hash,
		proofs: &ProofsMap,
		unwitnessed_events: &[H256],
		client: Arc<Client>,
	) -> Result<bool, Error> {
		let authorities: Vec<CryptoTypePublicPair> = client
			.runtime_api()
			.authorities(block) // client.info().best_hash
			.map_err(|e| Error::Other(e.to_string()))?
			.iter()
			.map(CryptoTypePublicPair::from)
			.collect();
		let target = (2 * ((authorities.len() - 1) / 3) + 1) as u16;
		for event in unwitnessed_events {
			let mut proof_count = 0;
			if proofs.contains_key(event) {
				let proof =
					proofs.get(event).ok_or(Error::Other("Empty ProofsMap given".to_string()))?;
				for key in proof.keys() {
					if !authorities.contains(key) {
						log::error!("received an event proof from an Unkown validator");
						return Ok(false)
					}
				}
				for (key, sig) in proof {
					let signature = Signature::from_slice(sig.as_slice())
						.ok_or(Error::Other("bad signature".to_string()))?;
					let pubkey = Public::from_slice(key.1.as_slice()).map_err(|_| {
						log::error!("bad public key provided for proof");
						Error::Other("bad public key".to_string())
					})?;
					if !pubkey.verify(&event, &signature) {
						log::error!("received faulty signature");
						return Ok(false)
					}
					proof_count += 1;
				}
				if proof_count < target {
					log::error!("Not Enough Proofs for event {:?}", event);
					return Ok(false)
				}
			} else {
				log::error!("didn't receive proof for event {:?}", event);
				return Ok(false)
			}
		}
		Ok(true)
	}

	async fn defer_block(&self, block_hash: <Block as BlockT>::Hash, unwitnessed_events: &[H256]) {
		let key = KademliaKey::new(&block_hash.encode());
		let mut inner = self.deferred_blocks.lock().await;

		let network_service = self.network_service.clone().await.unwrap();

		if inner.insert(block_hash, unwitnessed_events.into()).is_none() {
			log::info!(
				"⏭️  Deffered Block {} containing {} unwitnessed events",
				block_hash,
				unwitnessed_events.len()
			);
		}

		network_service.get_value(&key);
		log::info!("request sent to the dht to retreive proofs")
	}

	async fn provide_block(&self, block_hash: <Block as BlockT>::Hash, witnessed_events: &[H256]) {
		let network_service = self.network_service.clone().await.unwrap();

		if let Ok(proofs) = self.event_proofs.get_events_proofs(witnessed_events) {
			let key = KademliaKey::new(&block_hash.encode());
			match bincode::serialize(&proofs) {
				Ok(value) => {
					network_service.put_value(key, value);
				},
				Err(e) => log::error!("cant serialize proofs:{}", e),
			}
		}
	}
}

impl<Block: BlockT, I, Client, AuthorityId> WitnessBlockImport<Block, I, Client, AuthorityId> {
	#[cfg(feature = "on-chain-proofs")]
	pub fn new(parent_block_import: I) -> Self {
		Self { parent_block_import }
	}
	#[cfg(not(feature = "on-chain-proofs"))]
	pub fn new(
		parent_block_import: I,
		client: Arc<Client>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> Self {
		Self {
			parent_block_import,
			block_manager: Arc::new(BlockManager::new(client, event_proofs)),
		}
	}
}

#[async_trait::async_trait]
impl<
		Block: BlockT,
		I: BlockImport<Block> + Send + Sync,
		Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
		AuthorityId: Codec + Send + Sync + 'static,
	> BlockImport<Block> for WitnessBlockImport<Block, I, Client, AuthorityId>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
{
	type Error = ConsensusError;
	type Transaction = I::Transaction;

	async fn check_block(
		&mut self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		return self
			.parent_block_import
			.check_block(block)
			.await
			.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
	}

	#[cfg(feature = "on-chain-proofs")]
	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		return self
			.parent_block_import
			.import_block(block)
			.await
			.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
	}
	#[cfg(not(feature = "on-chain-proofs"))]
	async fn import_block(
		&mut self,
		block: BlockImportParams<Block, Self::Transaction>,
	) -> Result<ImportResult, Self::Error> {
		if let Some(block_extrinsics) = &block.body {
			let block_id = block.header.hash(); // self.block_manager.client.info().best_hash
			log::info!("number of extrinsics in block {}", block_extrinsics.len());
			let event_ids = self
				.block_manager
				.client
				.runtime_api()
				.get_extrinsic_ids(block_id, block_extrinsics)
				.ok()
				.unwrap_or_default();
			match verify_events_validity(
				self.block_manager.client.clone(),
				block_id,
				self.block_manager.event_proofs.clone(),
				event_ids.clone(),
			) {
				Ok(unwitnessed_ids) => {
					let block_hash = block.header.hash();
					if !unwitnessed_ids.is_empty() {
						for event in &unwitnessed_ids {
							match self.block_manager.event_proofs.get_proof_count(event.clone()) {
								Ok(count) => {
									log::info!(
										"unwitnessed event {} has proof count of {}",
										event,
										count
									);
								},
								Err(e) => {
									log::error!("{}", e);
								},
							}
						}
						self.block_manager.defer_block(block.header.hash(), &unwitnessed_ids).await;
						return Err(ConsensusError::ClientImport(
							"block contains unwitnessed events".to_string(),
						))
					} else {
						let parent_result = self.parent_block_import.import_block(block).await;
						match parent_result {
							Ok(result) => {
								if !event_ids.is_empty() {
									self.block_manager.provide_block(block_hash, &event_ids).await;
								}
								log::info!("📥 Block {} Imported", block_hash);
								return Ok(result)
							},
							Err(e) => return Err(ConsensusError::ClientImport(format!("{}", e))),
						}
					}
				},
				Err(e) => {
					log::error!("the following Error happened while verifying block events in the event_proofs:{}",e);
					return Err(ConsensusError::ClientImport(format!("{}", e)))
				},
			}
		} else {
			return self
				.parent_block_import
				.import_block(block)
				.await
				.map_err(|e| ConsensusError::ClientImport(format!("{}", e)))
		}
	}
}
