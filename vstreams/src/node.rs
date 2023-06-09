//! A helper for starting all the components needed to run a validated streams node

use crate::{
	chain_info::ChainInfo, configs::DebugLocalNetworkConfiguration, events::EventService,
	gossip::StreamsGossip, proofs::EventProofs, server,
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

	spawn_handle.clone().spawn_blocking("Event service", None, async move {
		let self_addr = DebugLocalNetworkConfiguration::self_multiaddr(gossip_port);
		let events_service = Arc::new(
			EventService::new(event_proofs, streams_gossip, keystore, tx_pool, client.clone())
				.await,
		);
		streams_gossip_service
			.start(spawn_handle.clone(), self_addr, peers, events_service.clone())
			.await;

		spawn_handle.spawn_blocking("gRPC server", None, async move {
			server::run(client, events_service, grpc_port).await.unwrap()
		});
	});
	Ok(())
}
