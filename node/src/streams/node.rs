use crate::{
	configs::DebugLocalNetworkConfiguration,
	service::FullClient,
	streams::{
		gossip::StreamsGossip, proofs::EventProofs, server::ValidatedStreamsGrpc,
		services::events::EventService,
	},
};
use node_runtime::opaque::Block;
use sc_service::{error::Error as ServiceError, SpawnTaskHandle};
use sc_transaction_pool::{BasicPool, FullChainApi};

use sp_keystore::CryptoStore;
use std::sync::Arc;

pub struct ValidatedStreamsNode {}
impl ValidatedStreamsNode {
	/// enables the current node to be a validated streams node by runing the core componenets
	/// which are the EventService, the StreamsGossip and the gRPC server.
	pub fn start(
		spawn_handle: SpawnTaskHandle,
		event_proofs: Arc<dyn EventProofs + Send + Sync>,
		client: Arc<FullClient>,
		keystore: Arc<dyn CryptoStore>,
		tx_pool: Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	) -> Result<(), ServiceError> {
		let (streams_gossip, streams_gossip_service) = StreamsGossip::create();

		spawn_handle.clone().spawn_blocking("Event service", None, async move {
			let self_addr = DebugLocalNetworkConfiguration::self_multiaddr();
			let peers = DebugLocalNetworkConfiguration::peers_multiaddrs(self_addr.clone());

			let events_service = Arc::new(
				EventService::new(event_proofs, streams_gossip, keystore, tx_pool, client).await,
			);

			streams_gossip_service
				.start(spawn_handle.clone(), self_addr, peers, events_service.clone())
				.await;

			spawn_handle.spawn_blocking("gRPC server", None, async move {
				ValidatedStreamsGrpc::run(events_service).await.unwrap()
			});
		});
		Ok(())
	}
}
