//! Helpers for starting up a validated streams node

use crate::{
	configs::{DebugLocalNetworkConfiguration, FullClient},
	gossip::StreamsGossip,
	proofs::EventProofs,
	server::ValidatedStreamsGrpc,
	services::events::EventService,
};
use libp2p::Multiaddr;
use node_runtime::opaque::Block;
use sc_service::{error::Error as ServiceError, SpawnTaskHandle};
use sc_transaction_pool::{BasicPool, FullChainApi};


use sp_keystore::Keystore;
use std::sync::Arc;

/// A helper for starting all the components needed to run a validated streams node
pub struct ValidatedStreamsNode {}
impl ValidatedStreamsNode {
	/// Starts the gossip, event service, and the gRPC server for the current validated streams
	/// node.
	pub fn start(
		spawn_handle: SpawnTaskHandle,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		client: Arc<FullClient>,
		keystore: Arc<dyn Keystore>,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
		grpc_port: u16,
		peers: Vec<Multiaddr>,
	) -> Result<(), ServiceError> {
		let (streams_gossip, streams_gossip_service) = StreamsGossip::create();

		spawn_handle.clone().spawn_blocking("Event service", None, async move {
			let self_addr = DebugLocalNetworkConfiguration::self_multiaddr();
			let events_service = Arc::new(
				EventService::new(event_proofs, streams_gossip, keystore, tx_pool, client.clone())
					.await,
			);
			streams_gossip_service
				.start(spawn_handle.clone(), self_addr, peers, events_service.clone())
				.await;

			spawn_handle.spawn_blocking("gRPC server", None, async move {
				ValidatedStreamsGrpc::run(client, events_service, grpc_port).await.unwrap()
			});
		});
		Ok(())
	}
}
