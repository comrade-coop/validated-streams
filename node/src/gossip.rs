use futures::{lock::Mutex, prelude::*, select, channel::mpsc::{Receiver, Sender}};
use libp2p::{
	core::{muxing::StreamMuxerBox, transport::Boxed},
	gossipsub::{self, Gossipsub, IdentTopic, MessageAuthenticity, GossipsubEvent},
	identity::{self, Keypair},
	swarm::SwarmEvent,
	Multiaddr, PeerId, Swarm,
};
use crate::{event_service::EventService, network_configs::LocalNetworkConfiguration};
use std::sync::Arc;
use serde::{Serialize,Deserialize};

pub struct Order (IdentTopic,Vec<u8>);

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct WitnessedEvent{
    pub signature: Vec<u8>,
    pub pub_key: Vec<u8>,
    pub event_id:String,
    pub extrinsic:Vec<u8>
}

pub struct StreamsGossip {
	pub key: Keypair,
	pub swarm: Arc<Mutex<Swarm<Gossipsub>>>,
}

impl StreamsGossip {
	pub async fn new() -> StreamsGossip {
		let key = StreamsGossip::create_keys();
		let transport = StreamsGossip::get_transport(key.clone()).await;
		let behavior = StreamsGossip::get_behavior(key.clone());
		let peer_id = StreamsGossip::get_peer_id(key.clone());
        log::info!("PEER ID: {:?}",peer_id);
		let swarm = Arc::new(Mutex::new(StreamsGossip::create_swarm(transport, behavior, peer_id)));
		StreamsGossip { key, swarm}
	}

	pub fn create_keys() -> Keypair {
		identity::Keypair::generate_ed25519()
	}

	pub fn get_peer_id(key: Keypair) -> PeerId {
		PeerId::from(key.public())
	}

	pub async fn get_transport(key: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
		libp2p::development_transport(key.clone())
			.await
			.expect("failed creating the transport")
	}

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
			match self.swarm.lock().await.dial(peer)
            {
                Err(e)=>{log::info!("Error dialing peer {:?}",e);},
                Ok(_)=>{log::info!("🤜🤛 Dialed Succefully");}
            }
		}
	}

	pub async fn subscribe(&self, topic: IdentTopic) {
		self.swarm.lock().await.behaviour_mut().subscribe(&topic).ok();
	}

    pub async fn publish(mut tx:Sender<Order>,topic:IdentTopic,message:Vec<u8>){
        tx.send(Order(topic,message)).await.unwrap_or_else(|e| log::error!("could not send order due to error:{:?}",e));
    }
    
    pub async fn listen(&self,addr:Multiaddr){
        let addr = self.swarm
            .lock()
            .await
            .listen_on(addr)
            .expect("failed listening on provided Address");
		log::info!("Listening on {:?}", addr);
    }

	pub async fn handle_incoming_messages(swarm: Arc<Mutex<Swarm<Gossipsub>>>,mut rc:Receiver<Order>,events_service:Arc<EventService>) {
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
                                        Ok(witnessed_event)=> events_service.handle_witnessed_event(witnessed_event).await,
                                        Err(e)=> log::error!("failed deserilizing message data due to error:{:?}",e),
                                    }
                            }
                            _ => {},
                        }
                    }
                    order = rc.select_next_some() =>{
                        match guard.behaviour_mut().publish(order.0, order.1){
                                Ok(id)=>{log::info!("Gossiped msg with id:{:?}",id)},
                                Err(e)=>{log::info!("Failed Gossiping message with Error: {:?}",e)}
                            }
                    }
            }
	    }
    }
    pub async fn start(&self,rc:Receiver<Order>,events_service:Arc<EventService>){
        let self_addr = LocalNetworkConfiguration::self_multiaddr();
        let peers = LocalNetworkConfiguration::peers_multiaddrs(self_addr.clone());
        self.listen(self_addr).await;
        self.dial_peers(peers.clone()).await;
        self.subscribe(IdentTopic::new("WitnessedEvent")).await;
        let swarm_clone = self.swarm.clone();
        
        tokio::spawn(async move{
            StreamsGossip::handle_incoming_messages(swarm_clone,rc,events_service).await;
        });

    }

    // test message delivery by making each node send a message with count that gets increased only when
    // all nodes have sent the message, after 5 iterations proceed to test handle_incoming_messages and publiSH
    // functions
/*    pub async fn run_test()*/
    /*{*/
        /*tokio::time::sleep(Duration::from_millis(2000)).await;*/
        /*let (tx,rc) = channel(32);*/
        /*let mut streams_gossip = StreamsGossip::new().await;*/
        /*let swarm_clone = streams_gossip.swarm.clone();*/
        /*let boot_nodes = LocalNetworkConfiguration::get_boot_nodes_multiaddrs();*/
        /*let self_addr = LocalNetworkConfiguration::get_self_multi_addr();*/
        /*let peers = LocalNetworkConfiguration::get_peers_multiaddrs(self_addr.clone());*/
        /*let self_addr_clone = self_addr.clone();*/
        /*let witnessed_stream: IdentTopic= gossipsub::IdentTopic::new("WitnessedStream");*/
        
        /*streams_gossip.listen(self_addr).await;*/
        /*streams_gossip.dial_peers(peers).await;*/
        /*streams_gossip.subscribe(witnessed_stream.clone()).await;*/
        /*let peer_id = StreamsGossip::get_peer_id(streams_gossip.key.clone()).to_base58();*/
        /*log::info!("peerd_id {}",peer_id);*/
        /*let mut count: u8 = 0; */
        /*let mut count_map :HashMap<u8,Vec<String>>= HashMap::new();*/
        /*let mut guard = streams_gossip.swarm.lock().await;    */
        /*loop{*/
        /*select! {*/
            /*_= tokio::time::sleep(Duration::from_millis(100)).fuse()=>{*/
                /*match guard.behaviour_mut().publish(witnessed_stream.clone(), vec![count]){*/
                        /*Ok(_)=>{},*/
                        /*Err(e)=>{log::error!("Failed Gossiping message with Error: {:?}",e)}*/
                        /*}*/
            /*},*/
            /*event = guard.select_next_some() =>{*/
                 /*match event{*/
                    /*SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),*/
                    /*SwarmEvent::Behaviour(GossipsubEvent::Message { propagation_source:_, message_id:_, message }) => */
                    /*{*/
                        /*let source= message.source.unwrap().to_base58();*/
                        /*let data :&u8= message.data.get(0).unwrap();*/
                        /*log::info!("peer:{:?} sent {:?}",source,data);*/
                        /*//why vec![peer_id] wont add the element in or_insert?*/
                        /*if count_map.entry(*data).or_insert(Vec::new()).contains(&peer_id)==false{*/
                            /*count_map.get_mut(data).unwrap().push(peer_id.clone());*/
                            /*match guard.behaviour_mut().publish(witnessed_stream.clone(), vec![count]){*/
                                     /*Ok(_)=>{},*/
                                     /*Err(e)=>{log::error!("Failed Gossiping message with Error: {:?}",e)}*/
                                /*}*/
                        /*}*/
                        /*if count_map.get(data).unwrap().contains(&source)==false{*/
                            /*count_map.get_mut(data).unwrap().push(source);*/
                            /*if count_map.get_mut(data).unwrap().len() == 4{*/
                                /*log::info!("received all gossiped messages of count {}",count);*/
                                /*count+=1;*/
                                /*if count == 5 {*/
                                    /*drop(guard);*/
                                    /*break*/
                                /*};*/
                            /*}*/
                        /*}*/
                    /*},*/
                    /*SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id:_, topic:_ }) => {}*/
                    /*_ => {},*/
                    /*}*/
                /*}*/
            /*}*/
        /*}*/
        /*tokio::spawn(async move{*/
                /*StreamsGossip::handle_incoming_messages(swarm_clone,rc).await;*/
        /*});*/
        /*loop{*/
                /*streams_gossip.publish(tx.clone(),witnessed_stream.clone(), vec![5,6,7]).await;*/
                /*tokio::time::sleep(Duration::from_millis(5000)).await;*/
        /*}*/
    /*}*/
}
