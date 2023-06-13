//! A module for gossiping messages with a swarm of peers.

use async_trait::async_trait;
use futures::{
	channel::mpsc::{channel, Receiver, Sender},
	prelude::*,
	select,
};
use libp2p::{
	core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
	gossipsub::{self, Gossipsub, GossipsubEvent, IdentTopic, MessageAuthenticity},
	identify::{Behaviour as Identify, Event as IdentifyEvent},
	identity::{self, Keypair},
	kad::{record::store::MemoryStore, Kademlia},
	mdns::tokio::Behaviour as MDns,
	mplex,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, tls, Multiaddr, PeerId, Swarm, Transport,
};

use std::sync::Arc;
#[cfg(test)]
pub mod tests;

#[derive(NetworkBehaviour)]
struct GossipNetworkBehavior {
	gossipsub: Gossipsub,
	kademlia: Kademlia<MemoryStore>,
	mdns: MDns,
	identify: Identify,
}

/// Represents an internal message passed between the public Gossip interface and the
/// internal GossipService handler
enum GossipOrder {
	SendMessage(IdentTopic, Vec<u8>),
	DialPeers(Vec<Multiaddr>),
	Listen(Multiaddr),
}

/// A struct which can be used to send messages to a libp2p gossipsub(+kademlia) network.
/// Cloning it is safe and reuses the same swarm and gossip network.
/// # Example Usage
/// ```
/// # use consensus_validated_streams::gossip::{Gossip, GossipHandler};
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
			swarm.behaviour_mut().gossipsub.subscribe(&topic).ok();
		}

		Self::run_loop(&mut swarm, self.rc, handler.as_ref()).await
	}

	/// Runs a select loop that handles events from the network and from orders
	async fn run_loop<H: GossipHandler + Send + Sync>(
		swarm: &mut Swarm<GossipNetworkBehavior>,
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
		swarm: &mut Swarm<GossipNetworkBehavior>,
		order: GossipOrder,
		handler: &H,
	) {
		match order {
			GossipOrder::SendMessage(topic, message) => {
				if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, message.clone()) {
					log::info!("Failed Gossiping message with Error: {:?}", e);
				}
				handler.handle(message).await;
				log::trace!("Gossiped a message!");
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
		swarm: &mut Swarm<GossipNetworkBehavior>,
		event: SwarmEvent<GossipNetworkBehaviorEvent, impl std::fmt::Display>,
		handler: &H,
	) {
		match event {
			SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),
			SwarmEvent::Behaviour(GossipNetworkBehaviorEvent::Gossipsub(
				GossipsubEvent::Subscribed { peer_id, topic },
			)) => {
				log::info!("{:?} subscribed to topic {:?}", peer_id, topic);
			},
			SwarmEvent::Behaviour(GossipNetworkBehaviorEvent::Gossipsub(
				GossipsubEvent::Message { message, .. },
			)) => {
				handler.handle(message.data).await;
			},
			SwarmEvent::Behaviour(GossipNetworkBehaviorEvent::Identify(
				IdentifyEvent::Received { info, peer_id },
			)) =>
				for addr in info.listen_addrs {
					swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
				},
			_ => {},
		}
	}

	/// Connects to a slice of peers
	fn dial_peers(swarm: &mut Swarm<GossipNetworkBehavior>, peers: &[Multiaddr]) {
		for peer in peers {
			log::trace!("Dialing peer {:?}", peer);
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
	fn create_swarm() -> Swarm<GossipNetworkBehavior> {
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
	fn get_behaviour(key: Keypair) -> GossipNetworkBehavior {
		let peer_id = PeerId::from(key.public());
		let gossipsub_config = gossipsub::GossipsubConfig::default();
		let mdns_config = libp2p::mdns::Config::default();
		let identify_config =
			libp2p::identify::Config::new("vstreams/1.0.0".to_string(), key.public());
		let message_authenticity = MessageAuthenticity::Signed(key);

		GossipNetworkBehavior {
			gossipsub: gossipsub::Gossipsub::new(message_authenticity, gossipsub_config).unwrap(),
			identify: Identify::new(identify_config),
			kademlia: Kademlia::new(peer_id, MemoryStore::new(peer_id)),
			mdns: MDns::new(mdns_config).expect("Failed to initialize mDNS"),
		}
	}
}
