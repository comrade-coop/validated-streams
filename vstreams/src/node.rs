//! A helper for starting all the components needed to run a validated streams node

use crate::{
	config::ValidatedStreamsNetworkConfiguration,
	events::{EventGossipHandler, EventValidator, EventWitnesser, EventServiceBlockState},
	gossip::StreamsGossip,
	proofs::EventProofsTrait,
	server,
};
use codec::Codec;
use futures::future;
use lru::LruCache;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockBackend, BlockchainEvents, HeaderBackend};
use sc_network::config::NetworkConfiguration;
use sc_service::{error::Error as ServiceError, SpawnTaskHandle};
use sc_transaction_pool_api::LocalTransactionPool;
use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_blockchain::HeaderMetadata;
use sp_consensus_aura::AuraApi;
use sp_keystore::CryptoStore;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::sync::{Arc, Mutex};
/// Starts the gossip, event service, and the gRPC server for the current validated streams node.
pub fn start<
	Block: BlockT,
	TxPool: LocalTransactionPool<Block = Block> + 'static,
	Client: Sync + Send + 'static,
	EventProofs: EventProofsTrait + Sync + Send + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
>(
	spawn_handle: SpawnTaskHandle,
	event_proofs: Arc<EventProofs>,
	client: Arc<Client>,
	keystore: Arc<dyn CryptoStore>,
	tx_pool: Arc<TxPool>,
	vs_network_configuration: ValidatedStreamsNetworkConfiguration,
	network_configuration: NetworkConfiguration,
    block_state: Arc<Mutex<LruCache<<Block as BlockT>::Hash,EventServiceBlockState>>>,
) -> Result<(), ServiceError>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client: HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
{
	let (streams_gossip, streams_gossip_service) = StreamsGossip::create();

	let event_gossip_handler =
		Arc::new(EventGossipHandler::new(block_state.clone(),client.clone(), event_proofs, tx_pool));

	let event_witnesser =
		Arc::new(EventWitnesser::new(client.clone(), streams_gossip.clone(), keystore, block_state.clone()));
	let event_validator = Arc::new(EventValidator::new(client));

	spawn_handle.spawn_blocking("Validated Streams gRPC server", None, async move {
		server::run(event_witnesser, event_validator, vs_network_configuration.grpc_addr)
			.await
			.unwrap()
	});

	spawn_handle.spawn_blocking("Validated Streams gossip", None, async move {
		future::join_all(
			network_configuration
				.listen_addresses
				.iter()
				.map(|a| vs_network_configuration.gossip_port.adjust_multiaddr(a.clone()))
				.map(|a| (streams_gossip.clone(), a)) // (eh.)
				.map(async move |(mut streams_gossip, a)| streams_gossip.listen(a).await),
		)
		.await;

		streams_gossip.clone().connect_to(network_configuration
			.boot_nodes
			.iter()
			.map(|a| vs_network_configuration.gossip_port.adjust_multiaddr(a.multiaddr.clone()))
			.map(|mut addr| {
				// Remove any /p2p/.. parts since we are using different keys
				match addr.pop() {
					Some(libp2p::core::multiaddr::Protocol::P2p(_)) => {},
					Some(x) => addr.push(x),
					None => {},
				}
				addr
			})
			.collect()).await;

		streams_gossip_service.run(event_gossip_handler).await;
	});

	Ok(())
}
