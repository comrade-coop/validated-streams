//! Block import which waits for all events to be witnessed before finalizing a block.
#![allow(unused_imports)]
use crate::{
	configs::FullClient,
	errors::Error,
	proofs::{EventProofs, ProofsMap},
	services::events::EventService,
};

use sc_client_api::HeaderBackend;
use sp_application_crypto::{RuntimePublic, CryptoTypePublicPair};
use futures::StreamExt;
use node_runtime::{self, opaque::Block, pallet_validated_streams::ExtrinsicDetails};
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
pub use sc_executor::NativeElseWasmExecutor;
use sc_network::{DhtEvent, Event, KademliaKey, NetworkDHTProvider, NetworkService, NetworkEventStream};
use sp_api::ProvideRuntimeApi;
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::AuraApi;
use sp_core::{
	sr25519::{Public, Signature},
	ByteArray, H256,
};
use sp_runtime::generic::BlockId;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
/// Wrapper around a [sc_consensus::BlockImport] which waits for all events to be witnessed in an
/// [EventProofs] instance before forwarding the block to the next import -- in effect preventing
/// the finalization for blocks that lack sufficient signatures from the gossip.
#[derive(Clone)]
pub struct WitnessBlockImport<I>
where
	I: BlockImport<Block>,
{
	parent_block_import: I,
	#[cfg(not(feature = "on-chain-proofs"))]
	pub block_manager: Arc<BlockManager>,
}
/// conatiner and manager of deferred blocks
#[cfg(not(feature = "on-chain-proofs"))]
pub struct BlockManager {
	/// list of deferred block and their corresponding unwitnessed_event
	pub deferred_blocks: Arc<Mutex<HashMap<H256, Vec<H256>>>>,
	/// provides access to the distributed hash table across all instances of the witness block
	/// import
	pub network_service: Arc<Mutex<Option<Arc<NetworkService<Block, H256>>>>>,
	client: Arc<FullClient>,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
}
#[cfg(not(feature = "on-chain-proofs"))]
impl BlockManager {
	/// handles incoming dht events and set the network service
	/// for all instances of the witness block import
	pub async fn handle_dht_events(
		dht: Arc<Mutex<Option<Arc<NetworkService<Block, H256>>>>>,
		inner_blocks: Arc<Mutex<HashMap<H256, Vec<H256>>>>,
		network_service: Arc<NetworkService<Block, H256>>,
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) {
		*dht.lock().await = Some(network_service.clone());
		let inner = inner_blocks.clone();
		tokio::spawn(async move {
			while let Some(event) = network_service.event_stream("event_proofs").next().await {
				if let Event::Dht(e) = event {
					match e {
						DhtEvent::ValueFound(values) =>
							Self::handle_found_proofs(
								values,
								inner.clone(),
								client.clone(),
								event_proofs.clone(),
							)
							.await,
						DhtEvent::ValueNotFound(key) => {
							// log::info!("block key not found in dht");
							let desrialized_key = H256::from_slice(key.to_vec().as_slice());
							inner.lock().await.remove(&desrialized_key);
						},
						DhtEvent::ValuePutFailed(_key)=>{
							// log::info!("value put failed for {:?}",key);
						},
						DhtEvent::ValuePut(key)=>{
							log::info!("key {:?} inserted in dht",key);
						},
					}
				}
			}
		});
	}
	async fn handle_found_proofs(
		values: Vec<(KademliaKey, Vec<u8>)>,
		deferred_blocks: Arc<Mutex<HashMap<H256, Vec<H256>>>>,
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) {
		for value in values {
			let mut inner = deferred_blocks.lock().await;
			let (key, value) = value;
			let key_vec = key.to_vec();
			if key_vec.len() == 32 {
				let desrialized_key = H256::from_slice(key_vec.as_slice());
				if inner.contains_key(&desrialized_key) {
					if let Ok(proofs) = bincode::deserialize::<ProofsMap>(&value) {
						let unwitnessed_events = inner.get(&desrialized_key).unwrap();
						if let Ok(result) =
							Self::verify_proofs(&proofs, unwitnessed_events, client.clone())
						{
							if result {
								log::info!(
									"💡 Retreived all event proofs of block {}",
									desrialized_key
								);
								event_proofs.add_events_proofs(proofs).ok();
								inner.remove(&desrialized_key);
							}
						} else {
							inner.remove(&desrialized_key);
						}
					} else {
						log::error!("failed deserializing proofs");
					}
				}
			} else {
				log::error!("bad block key length");
			}
		}
	}
	fn verify_proofs(
		proofs: &ProofsMap,
		unwitnessed_events: &[H256],
		client: Arc<FullClient>,
	) -> Result<bool, Error> {
		// let block_id = BlockId::Number(client.chain_info().best_number);
		let authorities: Vec<CryptoTypePublicPair> = client
			.runtime_api()
			.authorities(client.info().best_hash)
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
	async fn deffer_block(&self, block_hash: H256, unwitnessed_events: &[H256]) {
		let key = KademliaKey::new(&block_hash.as_bytes());
		let mut inner = self.deferred_blocks.lock().await;
		if let Some(dht) = &*self.network_service.lock().await {
			if inner.insert(block_hash, unwitnessed_events.into()).is_none() {
				log::info!(
					"⏭️  Deffered Block {} containing {} unwitnessed events",
					block_hash,
					unwitnessed_events.len()
				);
			}
			dht.get_value(&key);
			log::info!("request sent to the dht to retreive proofs")
		} else {
			log::error!("cant retreive block proofs, dht currently unavailable");
		}
	}
}

impl<I> WitnessBlockImport<I>
where
	I: BlockImport<Block>,
{
	#[cfg(feature = "on-chain-proofs")]
	pub fn new(parent_block_import: I) -> Self {
		Self { parent_block_import }
	}
	#[cfg(not(feature = "on-chain-proofs"))]
	pub fn new(
		parent_block_import: I,
		client: Arc<FullClient>,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
	) -> Self {
		let block_manager = Arc::new(BlockManager {
			deferred_blocks: Arc::new(Mutex::new(HashMap::new())),
			network_service: Arc::new(Mutex::new(None)),
			client,
			event_proofs,
		});
		Self { parent_block_import, block_manager }
	}
}
#[async_trait::async_trait]
impl<I: sc_consensus::BlockImport<Block>> sc_consensus::BlockImport<Block> for WitnessBlockImport<I>
where
	I: Send + Sync,
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
			// let block_id = BlockId::Number(self.block_manager.client.chain_info().best_number);
			log::info!("number of extrinsics in block {}",block_extrinsics.len());
			let event_ids = self
				.block_manager
				.client
				.runtime_api()
				.get_extrinsic_ids(self.block_manager.client.info().best_hash, block_extrinsics)
				.ok()
				.unwrap_or_default();
			match EventService::verify_events_validity(
				self.block_manager.client.clone(),
				self.block_manager.event_proofs.clone(),
				event_ids.clone(),
			) {
				Ok(unwitnessed_ids) =>{
					let block_hash = block.header.hash();
					if !unwitnessed_ids.is_empty() {
						for event in &unwitnessed_ids{
							match self.block_manager.event_proofs.get_proof_count(event.clone()){
								Ok(count) =>{
									log::info!("unwitnessed event {} has proof count of {}",event,count);
								}
								Err(e)=>{ log::error!("{}",e);}
							}
						}
						self.block_manager
							.deffer_block(block_hash, &unwitnessed_ids)
							.await;
						return Err(ConsensusError::ClientImport(
							"block contains unwitnessed events".to_string(),
						))
					} else {
						let parent_result =
							self.parent_block_import.import_block(block).await;
						match parent_result {
							Ok(result) => {
								let dht = self.block_manager.network_service.clone();
								if !event_ids.is_empty(){
									self.provide_block_proofs(dht, block_hash, &event_ids).await;
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
impl<I> WitnessBlockImport<I>
where
	I: sc_consensus::BlockImport<Block> + Sync,
{
	#[cfg(not(feature = "on-chain-proofs"))]
	async fn provide_block_proofs(
		&self,
		network_service: Arc<Mutex<Option<Arc<NetworkService<Block, H256>>>>>,
		block_hash: H256,
		event_ids: &[H256],
	) {
		if let Some(dht) = &*network_service.lock().await {
			if let Ok(proofs) = self.block_manager.event_proofs.get_events_proofs(event_ids) {
				let key = KademliaKey::new(&block_hash.as_bytes());
				match bincode::serialize(&proofs) {
					Ok(value) => {
						// log::info!("putting key {:?}", key);
						dht.put_value(key, value);
					},
					Err(e) => log::error!("cant serialize proofs:{}", e),
				}
			}
		} else {
			log::error!("cant provide block proofs, dht currently unavailable");
		}
	}
}
