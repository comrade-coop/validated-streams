use async_trait::async_trait;
use futures::{
	channel::mpsc::{channel, Receiver, Sender},
	prelude::*,
	select,
};
use libp2p::{
	core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
	gossipsub::{
		self, error::GossipsubHandlerError, Gossipsub, GossipsubEvent, IdentTopic,
		MessageAuthenticity,
	},
	identity::{self, Keypair},
	mplex,
	swarm::SwarmEvent,
	tcp, tls, Multiaddr, PeerId, Swarm, Transport,
};
use sc_service::SpawnTaskHandle;
use std::sync::Arc;
#[cfg(test)]
pub mod tests;
/// Represents an internal message passed between the public StreamsGossip interface and the
/// internal StreamsGossipService handler
enum StreamsGossipOrder {
	SendMessage(IdentTopic, Vec<u8>),
	DialPeers(Vec<Multiaddr>),
}

/// The interface to the gossip network
#[derive(Clone)]
pub struct StreamsGossip {
	tx: Sender<StreamsGossipOrder>,
}

/// The service maintaining the swarm and handling events of a StreamsGossip.
#[must_use]
pub struct StreamsGossipService {
	rc: Receiver<StreamsGossipOrder>,
}

#[async_trait]
pub trait StreamsGossipHandler {
	/// Returns the topics this StreamsGossipHandler is interested in. Note that changes in the
	/// output of this function will not be picked up.
	fn get_topics() -> Vec<IdentTopic>;
	/// Handles a message received on any of the topics this StreamsGossipHandler is Subscribed to.
	async fn handle(&self, message: Vec<u8>);
}

impl StreamsGossip {
	/// Creates a new StreamsGossip and a StreamsGossipService that can be used to start it.
	pub fn create() -> (Self, StreamsGossipService) {
		let (tx, rc) = channel(64); // TODO: make inbox size configurable?

		(Self { tx }, StreamsGossipService { rc })
	}

	/// Publishes a message on a specific topic to the libp2p swarm
	pub async fn publish(&mut self, topic: IdentTopic, message: Vec<u8>) {
		self.send_order(StreamsGossipOrder::SendMessage(topic, message)).await;
	}

	/// Connects to extra peers (aside from those passed to StreamsGossipService::start)
	#[allow(dead_code)]
	pub async fn connect_to(&mut self, peers: Vec<Multiaddr>) {
		self.send_order(StreamsGossipOrder::DialPeers(peers)).await;
	}

	/// Sends an order to the internal channel between the StreamsGossip and
	/// StreamsGossipService::run -- thus creating a rough Actor model out of the two.
	async fn send_order(&mut self, order: StreamsGossipOrder) {
		self.tx
			.send(order)
			.await
			.unwrap_or_else(|e| log::error!("could not send order due to error:{:?}", e));
	}
}

impl StreamsGossipService {
	/// Starts the gossip service using a spawn_handle, configuring its listen_addr and
	/// initial_peers, and passing all received messages to a handler
	// Subscribes to topics, dials the bootstrap peers, and starts listening for messages
	// Then runs spawns a background loop that handles incoming events
	pub async fn start<H: StreamsGossipHandler + Send + Sync + 'static>(
		self,
		spawn_handle: SpawnTaskHandle,
		listen_addr: Multiaddr,
		initial_peers: Vec<Multiaddr>,
		handler: Arc<H>,
	) {
		let mut swarm = Self::create_swarm();
		for topic in H::get_topics() {
			swarm.behaviour_mut().subscribe(&topic).ok();
		}
		let listen_addr =
			swarm.listen_on(listen_addr).expect("failed listening on provided Address");
		log::info!("Listening on {:?}", listen_addr);

		Self::dial_peers(&mut swarm, &initial_peers);

		spawn_handle.spawn_blocking("StreamsGossip", None, async move {
			Self::run_loop(&mut swarm, self.rc, handler.as_ref()).await;
		});
	}

	/// Runs a select loop that handles events from the network and from orders
	async fn run_loop<H: StreamsGossipHandler + Send + Sync>(
		swarm: &mut Swarm<Gossipsub>,
		mut rc: Receiver<StreamsGossipOrder>,
		handler: &H,
	) -> ! {
		loop {
			select! {
				order = rc.select_next_some() => Self::handle_incoming_order(swarm, order).await,
				event = swarm.select_next_some() => Self::handle_incoming_event(swarm, event, handler).await,
			}
		}
	}

	/// Handles an incomming channel order
	async fn handle_incoming_order(swarm: &mut Swarm<Gossipsub>, order: StreamsGossipOrder) {
		match order {
			StreamsGossipOrder::SendMessage(topic, message) => {
				if let Err(e) = swarm.behaviour_mut().publish(topic, message) {
					log::info!("Failed Gossiping message with Error: {:?}", e);
				}
			},
			StreamsGossipOrder::DialPeers(peers) => {
				Self::dial_peers(swarm, &peers);
			},
		}
	}

	/// Handles an incomming swarm event, passing message data to the handler
	async fn handle_incoming_event<H: StreamsGossipHandler + Send>(
		_swarm: &mut Swarm<Gossipsub>,
		event: SwarmEvent<GossipsubEvent, GossipsubHandlerError>,
		handler: &H,
	) {
		match event {
			SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),
			SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id, topic}) => {
				log::info!("{:?} subscribed to topic {:?}",peer_id, topic);
			},
			SwarmEvent::Behaviour(GossipsubEvent::Message {
				propagation_source: _,
				message_id: _,
				message,
			}) => {
				handler.handle(message.data).await;
			},
			_ => {}
			// SwarmEvent::Behaviour(GossipsubEvent::Unsubscribed {peer_id, topic}) =>log::info!("peer {:?} unsibscribed from topic{:?}",peer_id,topic),
			// SwarmEvent::Behaviour(GossipsubEvent::GossipsubNotSupported{peer_id}) =>log::info!("GossipsubNotSupported {:?}",peer_id),
			// SwarmEvent::ConnectionClosed { peer_id, endpoint:_, num_established:_, cause } => log::info!("connection closed with :{} with cause{:?}",peer_id,cause),
			// SwarmEvent::IncomingConnection { local_addr, send_back_addr } => log::info!("incoming connection :{} {}",local_addr,send_back_addr),
			// SwarmEvent::IncomingConnectionError { local_addr, send_back_addr:_, error } => log::info!("incoming connection error:{:?} with error{:?}",local_addr,error),
			// SwarmEvent::OutgoingConnectionError { peer_id, error } => log::info!("outgoing connection error with:{:?} with error {:?}",peer_id,error),
			// SwarmEvent::BannedPeer { peer_id, endpoint:_} => log::info!("Bannned peer :{}",peer_id),
			// SwarmEvent::ExpiredListenAddr { listener_id, address } => log::info!("Expired listen addr:{:?} and address {:?}",listener_id,address),
			// SwarmEvent::ListenerClosed { listener_id, addresses, reason } => log::info!("listner closed:{:?} {:?} with reason {:?}",listener_id,addresses,reason),
			// SwarmEvent::ListenerError { listener_id, error } => log::info!("listener error:{:?} with error {:?}",listener_id,error),
			// SwarmEvent::Dialing(_) => log::info!("dialing"),
		}
	}

	/// Connects to a slice of peers
	fn dial_peers(swarm: &mut Swarm<Gossipsub>, peers: &[Multiaddr]) {
		for peer in peers {
			match swarm.dial(peer.clone()) {
				Err(e) => {
					log::info!("Error dialing peer {:?}", e);
				},
				Ok(_) => {
					log::info!("🤜🤛 Dialed Succefully");
				},
			}
		}
	}

	/// Creates a new gossipsub swarm
	fn create_swarm() -> Swarm<Gossipsub> {
		let key = Self::create_keys();
		let transport = Self::get_transport(key.clone());
		let behaviour = Self::get_behaviour(key.clone());
		let peer_id = PeerId::from(key.public());
		log::info!("PEER ID: {:?}", peer_id);
		libp2p::Swarm::with_threadpool_executor(transport, behaviour, peer_id)
	}

	/// Creates a ed255519 keypair for the swarm
	fn create_keys() -> Keypair {
		identity::Keypair::generate_ed25519()
	}

	/// Creates a tcp transport over mplex and tls
	fn get_transport(key: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
		tcp::async_io::Transport::new(tcp::Config::default())
			.upgrade(upgrade::Version::V1)
			.authenticate(tls::Config::new(&key).expect("Failed using tls keys"))
			.multiplex(mplex::MplexConfig::new())
			.boxed()
	}

	/// Creates a gossipsub behaviour
	fn get_behaviour(key: Keypair) -> Gossipsub {
		let message_authenticity = MessageAuthenticity::Signed(key);
		let gossipsub_config = gossipsub::GossipsubConfig::default();
		gossipsub::Gossipsub::new(message_authenticity, gossipsub_config).unwrap()
	}
}
