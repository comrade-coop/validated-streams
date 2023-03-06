use super::{StreamsGossip, StreamsGossipHandler};
use crate::streams::proofs::WitnessedEvent;
use async_trait::async_trait;
use libp2p::{gossipsub::IdentTopic, Multiaddr};
use sc_service::TaskManager;
use std::{
	sync::{Arc, Mutex},
	time::Duration,
};
pub struct MockGossipHandler {
	messages: Mutex<Vec<WitnessedEvent>>,
}
#[async_trait]
impl StreamsGossipHandler for MockGossipHandler {
	fn get_topics() -> Vec<libp2p::gossipsub::IdentTopic> {
		vec![IdentTopic::new("WitnessedEvent")]
	}

	async fn handle(&self, message: Vec<u8>) {
		match bincode::deserialize::<WitnessedEvent>(message.as_slice()) {
			Ok(witnessed_event) => {
				self.messages.lock().unwrap().push(witnessed_event);
			},
			Err(e) => log::error!("failed deserilizing message data due to error:{:?}", e),
		}
	}
}
/// test receiving messages from other peers by creating a mock service that listens on a different
/// Multiaddr and test that messages sent from self should not be received
/// which means the length of messages should be 1 (because the StreamsGossipHandler would be )
#[tokio::test]
pub async fn test_self_message() {
	let (mut streams_gossip, service) = StreamsGossip::create();
	let (_, mock_peer_service) = StreamsGossip::create();
	let tokio_handle = tokio::runtime::Handle::current();
	let task_manager = TaskManager::new(tokio_handle, None).unwrap();
	let spawn_handle = task_manager.spawn_handle();
	let peer_spawn_handle = task_manager.spawn_handle();
	let self_addr: Multiaddr = "/ip4/127.0.0.1/tcp/10001".to_string().parse().unwrap();
	let peer_mock_addr: Multiaddr = "/ip4/127.0.0.1/tcp/10002".to_string().parse().unwrap();
	let mock_handler = Arc::new(MockGossipHandler { messages: Mutex::new(Vec::new()) });
	let witnessed_event = create_witnessed_event();
	//connections to self should be rejected
	task_manager.spawn_handle().spawn(
		"Test",
		None,
		service.start(
			spawn_handle,
			self_addr.clone(),
			vec![self_addr.clone()],
			mock_handler.clone(),
		),
	);
	task_manager.spawn_handle().spawn(
		"Test2",
		None,
		mock_peer_service.start(
			peer_spawn_handle,
			peer_mock_addr.clone(),
			vec![],
			mock_handler.clone(),
		),
	);

	// wait for the two peers to start
	tokio::time::sleep(Duration::from_millis(1000)).await;
	streams_gossip.connect_to(vec![peer_mock_addr.clone()]).await;

	//wait for connection to be established between peers
	tokio::time::sleep(Duration::from_millis(1000)).await;
	streams_gossip
		.publish(IdentTopic::new("WitnessedEvent"), bincode::serialize(&witnessed_event).unwrap())
		.await;

	//wait for message to be received by the other peer
	tokio::time::sleep(Duration::from_millis(1000)).await;
	assert!(mock_handler.messages.lock().unwrap().len() == 1);
	assert_eq!(mock_handler.messages.lock().unwrap().get(0).unwrap(), &witnessed_event);
}
fn create_witnessed_event() -> WitnessedEvent {
	WitnessedEvent { event_id: sp_core::H256::repeat_byte(0), pub_key: vec![], signature: vec![] }
}
