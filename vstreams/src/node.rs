//! A helper for starting all the components needed to run a validated streams node

use crate::{
	configs::DebugLocalNetworkConfiguration,
	events::{EventGossipHandler, EventValidator, EventWitnesser},
	gossip::StreamsGossip,
	proofs::EventProofs,
	server,
};
use codec::Codec;
use libp2p::Multiaddr;
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockBackend, BlockchainEvents, HeaderBackend};
use sc_service::{error::Error as ServiceError, SpawnTaskHandle};
use sc_transaction_pool_api::LocalTransactionPool;
use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_blockchain::HeaderMetadata;
use sp_consensus_aura::AuraApi;
use sp_keystore::CryptoStore;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use std::sync::Arc;

/// Starts the gossip, event service, and the gRPC server for the current validated streams node.
pub fn start<
	Block: BlockT,
	TxPool,
	Client: Sync + Send + 'static,
	AuthorityId: Codec + Send + Sync + 'static,
>(
	spawn_handle: SpawnTaskHandle,
	event_proofs: Arc<dyn EventProofs + Send + Sync>,
	client: Arc<Client>,
	keystore: Arc<dyn CryptoStore>,
	tx_pool: Arc<TxPool>,
	grpc_port: u16,
	gossip_port: u16,
	peers: Vec<Multiaddr>,
) -> Result<(), ServiceError>
where
	CryptoTypePublicPair: for<'a> From<&'a AuthorityId>,
	Client: ChainInfo<Block>
		+ HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>,
	Client::Api: ExtrinsicDetails<Block> + AuraApi<Block, AuthorityId>,
	TxPool: LocalTransactionPool<Block = Block> + 'static,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
{
	let (streams_gossip, streams_gossip_service) = StreamsGossip::create();

	let self_addr = DebugLocalNetworkConfiguration::self_multiaddr(gossip_port);
	let event_gossip_handler =
		Arc::new(EventGossipHandler::new(client.clone(), event_proofs, tx_pool));

	let event_witnesser = Arc::new(EventWitnesser::new(client.clone(), streams_gossip, keystore));
	let event_validator = Arc::new(EventValidator::new(client));

	spawn_handle.spawn_blocking("gRPC server", None, async move {
		server::run(event_witnesser, event_validator, grpc_port).await.unwrap()
	});

	spawn_handle.spawn_blocking("Events service", None, async move {
		streams_gossip_service.run(self_addr, peers, event_gossip_handler).await;
	});

	Ok(())
}
