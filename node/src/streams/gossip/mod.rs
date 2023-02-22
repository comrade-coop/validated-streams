use crate::streams::{
	configs::LocalNetworkConfiguration,
	services::events::{EventService, WitnessedEvent},
};
use futures::{
	channel::mpsc::{Receiver, Sender},
	lock::Mutex,
	prelude::*,
	select,
};
use libp2p::{
	core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
	gossipsub::{self, Gossipsub, GossipsubEvent, IdentTopic, MessageAuthenticity},
	identity::{self, Keypair},
	mplex,
	swarm::SwarmEvent,
	tcp, tls, Multiaddr, PeerId, Swarm, Transport,
};
use std::sync::Arc;
/// represents a topic and a serialized message to be sent over the network
pub struct Order(IdentTopic, Vec<u8>);
pub struct StreamsGossip {
	pub key: Keypair,
	pub swarm: Arc<Mutex<Swarm<Gossipsub>>>,
}

impl StreamsGossip {
	/// creates a new StreamsGossip that uses the tcp transport over mplex and tls
	/// with gossipsub behaviour
	pub async fn new() -> StreamsGossip {
		let key = StreamsGossip::create_keys();
		let transport = StreamsGossip::get_transport(key.clone()).await;
		let behavior = StreamsGossip::get_behavior(key.clone());
		let peer_id = StreamsGossip::get_peer_id(key.clone());
		log::info!("PEER ID: {:?}", peer_id);
		let swarm = Arc::new(Mutex::new(StreamsGossip::create_swarm(transport, behavior, peer_id)));
		StreamsGossip { key, swarm }
	}
	/// creates ed255519 keypair
	pub fn create_keys() -> Keypair {
		identity::Keypair::generate_ed25519()
	}
	/// retreive the peerid from the given keypair
	pub fn get_peer_id(key: Keypair) -> PeerId {
		PeerId::from(key.public())
	}
	/// create a tcp transport over mplex and tls
	pub async fn get_transport(key: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
		tcp::async_io::Transport::new(tcp::Config::default())
			.upgrade(upgrade::Version::V1)
			.authenticate(tls::Config::new(&key).expect("Failed using tls keys"))
			.multiplex(mplex::MplexConfig::new())
			.boxed()
	}
	/// creates a gossipsub behaviour
	pub fn get_behavior(key: Keypair) -> Gossipsub {
		let message_authenticity = MessageAuthenticity::Signed(key);
		// set default parameters for gossipsub
		let gossipsub_config = gossipsub::GossipsubConfig::default();
		// build a gossipsub network behaviour
		gossipsub::Gossipsub::new(message_authenticity, gossipsub_config).unwrap()
	}

	pub fn create_swarm(
		transport: Boxed<(PeerId, StreamMuxerBox)>,
		behaviour: Gossipsub,
		peer_id: PeerId,
	) -> Swarm<Gossipsub> {
		libp2p::Swarm::with_threadpool_executor(transport, behaviour, peer_id)
	}

	pub async fn dial_peers(&self, peers: Vec<Multiaddr>) {
		for peer in peers {
			match self.swarm.lock().await.dial(peer) {
				Err(e) => {
					log::info!("Error dialing peer {:?}", e);
				},
				Ok(_) => {
					log::info!("🤜🤛 Dialed Succefully");
				},
			}
		}
	}

	pub async fn subscribe(&self, topic: IdentTopic) {
		self.swarm.lock().await.behaviour_mut().subscribe(&topic).ok();
	}
	/// publish a message to the network by sending an order to the swarm via an mpsc channel
	pub async fn publish(mut tx: Sender<Order>, topic: IdentTopic, message: Vec<u8>) {
		tx.send(Order(topic, message))
			.await
			.unwrap_or_else(|e| log::error!("could not send order due to error:{:?}", e));
	}

	pub async fn listen(&self, addr: Multiaddr) {
		let addr = self
			.swarm
			.lock()
			.await
			.listen_on(addr)
			.expect("failed listening on provided Address");
		log::info!("Listening on {:?}", addr);
	}
	/// starts an event loop in another thread that receives diffrent events both from the network
	/// and events to gossip messages via an mpsc channel, in order to use this function,
	/// you need to pass it a GossipSub swarm and an mpsc receiver in order to gossip messages and
	/// an event service for handling WitnessedEvent messages.
	async fn handle_incoming_messages(
		swarm: Arc<Mutex<Swarm<Gossipsub>>>,
		mut rc: Receiver<Order>,
		events_service: Arc<EventService>,
	) {
		loop {
			let mut guard = swarm.lock().await;
			select! {
					event = guard.select_next_some() =>
					{
						match event{
							SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),
							SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id:_, topic:_ }) => {}
							SwarmEvent::Behaviour(GossipsubEvent::Message { propagation_source:_, message_id:_, message }) =>{
									match bincode::deserialize::<WitnessedEvent>(message.data.as_slice()){
										Ok(witnessed_event)=> {events_service.handle_witnessed_event(witnessed_event).await.ok();},
										Err(e)=> log::error!("failed deserilizing message data due to error:{:?}",e),
									}
							}
							_ => {},
						}
					}
					order = rc.select_next_some() =>{
						if let Err(e)= guard.behaviour_mut().publish(order.0, order.1){
							log::info!("Failed Gossiping message with Error: {:?}",e);
					}
				}
			}
		}
	}
	/// start listening for  incoming messages and dial the bootstrap peers from
	/// the NetworkConfiguration and subscribes to the WitnessedEvent topic
	pub async fn start(&self, rc: Receiver<Order>, events_service: Arc<EventService>) {
		let self_addr = LocalNetworkConfiguration::self_multiaddr();
		let peers = LocalNetworkConfiguration::peers_multiaddrs(self_addr.clone());
		self.listen(self_addr).await;
		self.dial_peers(peers.clone()).await;
		self.subscribe(IdentTopic::new("WitnessedEvent")).await;
		let swarm_clone = self.swarm.clone();

		tokio::spawn(async move {
			StreamsGossip::handle_incoming_messages(swarm_clone, rc, events_service).await;
		});
	}
}
