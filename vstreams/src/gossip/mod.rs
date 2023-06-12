//! A module for gossiping messages with a swarm of peers.

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
use std::sync::Arc;
#[cfg(test)]
pub mod tests;

/// Represents an internal message passed between the public Gossip interface and the
/// internal GossipService handler
enum GossipOrder {
	SendMessage(IdentTopic, Vec<u8>),
	DialPeers(Vec<Multiaddr>),
	Listen(Multiaddr),
}

/// A struct which can be used to send messages to a libp2p gossipsub network.
/// Cloning it is safe and reuses the same swarm and gossip network.
/// # Example Usage
/// ```
/// # use vstreams::gossip::{Gossip, GossipHandler};
/// # use std::sync::Arc;
/// # use async_trait::async_trait;
/// use libp2p::gossipsub::IdentTopic;
/// struct ExampleHandler {}
/// #[async_trait]
/// impl GossipHandler for ExampleHandler {
///     fn get_topics() -> Vec<IdentTopic> { vec!(IdentTopic::new("some_topic")) }
///     async fn handle(&self, message: Vec<u8>) {
///         println!("Received message! {:?}", message);
///     }
/// }
/// # async fn async_stuff() { // Only doctest compilation, as actual usage blocks forever
/// let (gossip, service) = Gossip::create();
/// gossip.clone().listen("/ip4/0.0.0.0/tcp/10000".parse().unwrap());
/// gossip.clone().connect_to(vec![ "/ip4/0.0.0.0/tcp/10001".parse().unwrap() ]);
/// tokio::spawn(async move {
///     service.run(Arc::new(ExampleHandler {})).await;
/// });
/// // Later...
/// gossip.clone().publish(IdentTopic::new("some_topic"), vec!(0, 1, 2, 3)).await;
/// # }
/// ```
#[derive(Clone)]
pub struct Gossip {
	tx: Sender<GossipOrder>,
}

/// A handle used to start the networking code of a [Gossip].
#[must_use]
pub struct GossipService {
	rc: Receiver<GossipOrder>,
}

/// A handler for all messages received or sent by a [Gossip]
#[async_trait]
pub trait GossipHandler {
	/// Returns the list of topics the [GossipHandler] is interested in. Note that changes in the
	/// output of this function will not be picked up.
	fn get_topics() -> Vec<IdentTopic>;

	/// Handles a message received on any of the topics this [GossipHandler] is subscribed to,
	/// *or* a message sent by the [Gossip] to other peers.
	/// Currently, messages are not differenciated by topic or origin.
	async fn handle(&self, message: Vec<u8>);
}

impl Gossip {
	/// Creates a new [Gossip] and a [GossipService] that can be used to start it.
	pub fn create() -> (Self, GossipService) {
		let (tx, rc) = channel(64); // TODO: make inbox size configurable?

		(Self { tx }, GossipService { rc })
	}

	/// Publishes a message to peers subscribed to a specific topic
	pub async fn publish(&mut self, topic: IdentTopic, message: Vec<u8>) {
		self.send_order(GossipOrder::SendMessage(topic, message)).await;
	}

	/// Connects to a list of peers
	pub async fn connect_to(&mut self, peers: Vec<Multiaddr>) {
		self.send_order(GossipOrder::DialPeers(peers)).await;
	}

	/// Listen on an address
	pub async fn listen(&mut self, address: Multiaddr) {
		self.send_order(GossipOrder::Listen(address)).await;
	}

	/// Send an order to the internal channel between the Gossip and
	/// GossipService::run -- creating an "Actor" model out of the two.
	async fn send_order(&mut self, order: GossipOrder) {
		self.tx
			.send(order)
			.await
			.unwrap_or_else(|e| log::error!("could not send order due to error:{:?}", e));
	}
}

impl GossipService {
	/// Starts the gossip service. This function never returns, so make sure to spawn it as a
	/// separate task.
	pub async fn run<H: GossipHandler + Send + Sync + 'static>(self, handler: Arc<H>) -> ! {
		let mut swarm = Self::create_swarm();

		for topic in H::get_topics() {
			swarm.behaviour_mut().subscribe(&topic).ok();
		}

		Self::run_loop(&mut swarm, self.rc, handler.as_ref()).await
	}

	/// Runs a select loop that handles events from the network and from orders
	async fn run_loop<H: GossipHandler + Send + Sync>(
		swarm: &mut Swarm<Gossipsub>,
		mut rc: Receiver<GossipOrder>,
		handler: &H,
	) -> ! {
		loop {
			select! {
				order = rc.select_next_some() => Self::handle_incoming_order(swarm, order, handler).await,
				event = swarm.select_next_some() => Self::handle_incoming_event(swarm, event, handler).await,
			}
		}
	}

	/// Handles an incoming channel order
	async fn handle_incoming_order<H: GossipHandler + Send>(
		swarm: &mut Swarm<Gossipsub>,
		order: GossipOrder,
		handler: &H,
	) {
		match order {
			GossipOrder::SendMessage(topic, message) => {
				if let Err(e) = swarm.behaviour_mut().publish(topic, message.clone()) {
					log::info!("Failed Gossiping message with Error: {:?}", e);
				}
				handler.handle(message).await;
			},
			GossipOrder::DialPeers(peers) => {
				Self::dial_peers(swarm, &peers);
			},
			GossipOrder::Listen(listen_addr) => {
				log::info!("Listening on {:?}", listen_addr);
				if let Err(e) = swarm.listen_on(listen_addr) {
					log::info!("Failed listening on provided Address: {:?}", e);
				}
			},
		}
	}

	/// Handles an incoming swarm event, passing message data to the handler
	async fn handle_incoming_event<H: GossipHandler + Send>(
		_swarm: &mut Swarm<Gossipsub>,
		event: SwarmEvent<GossipsubEvent, GossipsubHandlerError>,
		handler: &H,
	) {
		match event {
			SwarmEvent::NewListenAddr { address, .. } => log::debug!("Listening on {:?}", address),
			SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id, topic }) => {
				log::debug!("{:?} subscribed to topic {:?}", peer_id, topic);
			},
			SwarmEvent::Behaviour(GossipsubEvent::Message { message, .. }) => {
				handler.handle(message.data).await;
			},
			_ => {},
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
		log::info!("Validated Streams Gossip peer ID: {:?}", peer_id);
		libp2p::Swarm::with_threadpool_executor(transport, behaviour, peer_id)
	}

	/// Creates a ed255519 nodekey for the swarm
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

	/// Assembles a gossipsub behaviour
	fn get_behaviour(key: Keypair) -> Gossipsub {
		let message_authenticity = MessageAuthenticity::Signed(key);
		let gossipsub_config = gossipsub::GossipsubConfig::default();
		gossipsub::Gossipsub::new(message_authenticity, gossipsub_config).unwrap()
	}
}
