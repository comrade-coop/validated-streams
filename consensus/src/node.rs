//! A helper for starting all the components needed to run a full Validated Streams node

use crate::{
	config::ValidatedStreamsNetworkConfiguration,
	events::{EventGossipHandler, EventValidator, EventWitnesser},
	gossip::Gossip,
	proofs::EventProofsTrait,
	server,
};
use codec::Codec;
use futures::future;
use pallet_validated_streams::ValidatedStreamsApi;
use sc_client_api::{BlockBackend, BlockchainEvents, HeaderBackend};
use sc_network::config::NetworkConfiguration;
use sc_service::{error::Error as ServiceError, SpawnTaskHandle};
use sc_transaction_pool_api::LocalTransactionPool;
use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_blockchain::HeaderMetadata;
use sp_consensus_aura::AuraApi;
use sp_keystore::CryptoStore;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::sync::Arc;

/// Start all the services of the Validated Streams node.
/// This functions starts the gossip, event service, and the gRPC server for the current node, and
/// configures their ports using the passed configuration.
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
) -> Result<(), ServiceError>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client: HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>,
	Client::Api: ValidatedStreamsApi<Block> + AuraApi<Block, AuthorityId>,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
{
	let (streams_gossip, streams_gossip_service) = Gossip::create();

	let event_gossip_handler =
		Arc::new(EventGossipHandler::new(client.clone(), event_proofs, tx_pool));

	let event_witnesser =
		Arc::new(EventWitnesser::new(client.clone(), streams_gossip.clone(), keystore));
	let event_validator = Arc::new(EventValidator::new(client));

	spawn_handle.spawn_blocking("Validated Streams gRPC server", None, async move {
		server::run(event_witnesser, event_validator, vs_network_configuration.grpc_addr)
			.await
			.unwrap()
	});

	let gossip_listen_addresses = network_configuration
		.listen_addresses
		.iter()
		.map(|addr| vs_network_configuration.gossip_port.adjust_multiaddr(addr.clone()))
		.collect::<Vec<_>>();

	let gossip_peers = if vs_network_configuration.gossip_bootnodes.is_empty() {
		vs_network_configuration.gossip_bootnodes
	} else {
		network_configuration
			.boot_nodes
			.iter()
			.map(|addr| {
				let mut addr =
					vs_network_configuration.gossip_port.adjust_multiaddr(addr.multiaddr.clone());
				// Remove the final /p2p/.. part as we are using different keys for gossip
				match addr.pop() {
					Some(libp2p::core::multiaddr::Protocol::P2p(_)) => {},
					Some(x) => addr.push(x),
					None => {},
				}
				addr
			})
			.collect()
	};

	spawn_handle.spawn_blocking("Validated Streams gossip", None, async move {
		future::join_all(
			gossip_listen_addresses
				.into_iter()
				.map(|a| (streams_gossip.clone(), a)) // (eh.)
				.map(async move |(mut streams_gossip, addr)| streams_gossip.listen(addr).await),
		)
		.await;

		streams_gossip.clone().connect_to(gossip_peers).await;

		streams_gossip_service.run(event_gossip_handler).await;
	});

	Ok(())
}
