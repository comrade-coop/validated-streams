use futures::{lock::Mutex, prelude::*, select};

use libp2p::{
	core::{muxing::StreamMuxerBox, transport::Boxed},
	gossipsub::{self, Gossipsub, IdentTopic, MessageAuthenticity, GossipsubEvent},
	identity::{self, Keypair},
	swarm::SwarmEvent,
	Multiaddr, PeerId, Swarm,
};

use std::{sync::Arc, time::Duration, future, collections::HashMap};

use crate::network_configs::LocalNetworkConfiguration;


pub struct StreamsGossip {
	key: Keypair,
	swarm: Arc<Mutex<Swarm<Gossipsub>>>,
}

impl StreamsGossip {
	pub async fn new() -> StreamsGossip {
		let key = StreamsGossip::create_keys();
		let transport = StreamsGossip::get_transport(key.clone()).await;
		let behavior = StreamsGossip::get_behavior(key.clone());
		let peer_id = StreamsGossip::get_peer_id(key.clone());
        log::info!("PEER ID: {:?}",peer_id);
		let swarm = Arc::new(Mutex::new(StreamsGossip::create_swarm(transport, behavior, peer_id)));
		StreamsGossip { key, swarm }
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
                Ok(_)=>{log::info!("Dialed Succefully");}
            }
		}
	}

	pub async fn subscribe(&self, topic: IdentTopic) {
		self.swarm.lock().await.behaviour_mut().subscribe(&topic).ok();
	}

    pub async fn publish(&mut self,topic:IdentTopic,message:Vec<u8>){
        match self.swarm.lock().await.behaviour_mut().publish(topic, message){
            Ok(id)=>{log::info!("Gossiped msg with id:{:?}",id)},
            Err(e)=>{log::info!("Failed Gossiping message with Error: {:?}",e)}
        }
    }
    
    pub async fn listen(&self,addr:Multiaddr){
        let addr = self.swarm
            .lock()
            .await
            .listen_on(addr)
            .expect("failed listening on provided Address");
		log::info!("Listening on {:?}", addr);
    }

    //used the select! instead of awaiting (select_next_some().await) directly to 
    //prevent holding on the swarm since its used for both listening and publishing messages
    //it drops the guard every 500 milliseconds via a mock asynch operation (sleeping)
	pub async fn handle_incoming_messages(swarm: Arc<Mutex<Swarm<Gossipsub>>>) {
        loop {
            let mut guard = swarm.lock().await;
            select! {
                    event = guard.select_next_some() =>
                    {
                        match event{
                            SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),
                            SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id:_, topic:_ }) => {}
                            SwarmEvent::Behaviour(GossipsubEvent::Message { propagation_source:_, message_id:_, message }) =>{
                                let source= message.source.unwrap().to_base58();
                                let data :&u8= message.data.get(0).unwrap();
                                log::info!("peer:{:?} sent {:?}",source,data);

                            } 
                            _ => {},
                        }
                    }
                    // used to drop the guard periodically to avoid starvation on swarm
                    // sleeping is necessary to avoid immediate lock acuiring at the
                    // start of the loop
					_ = tokio::time::sleep(Duration::from_millis(500)).fuse()=>{
						drop(guard);
                        tokio::time::sleep(Duration::from_millis(100)).await;
					}
            }
	    }
    }
	pub async fn mock_order()-> future::Ready<()> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        future::ready(())
    }

    // test message delivery by making each node send a message with count that gets increased only when
    // all nodes have sent the message, after 5 iterations proceed to test handle_incoming_messages and publiSH
    // functions
    pub async fn run_test()
    {
        tokio::time::sleep(Duration::from_millis(2000)).await;
        let mut streams_gossip = StreamsGossip::new().await;
        let swarm_clone = streams_gossip.swarm.clone();
        let peers = LocalNetworkConfiguration::get_peers_multi_addresses();
        let self_addr = LocalNetworkConfiguration::get_self_multi_addr();
        let self_addr_clone = self_addr.clone();
        let witnessed_stream: IdentTopic= gossipsub::IdentTopic::new("WitnessedStream");
        
        streams_gossip.listen(self_addr).await;
        streams_gossip.dial_peers(peers.into_iter().filter(|peer| *peer != self_addr_clone).collect()).await;
        streams_gossip.subscribe(witnessed_stream.clone()).await;
        let peer_id = StreamsGossip::get_peer_id(streams_gossip.key.clone()).to_base58();
        log::info!("peerd_id {}",peer_id);
        let mut count: u8 = 0; 
        let mut count_map :HashMap<u8,Vec<String>>= HashMap::new();
        let mut guard = streams_gossip.swarm.lock().await;    
        loop{
        select! {
            _= StreamsGossip::mock_order().fuse()=>{
                match guard.behaviour_mut().publish(witnessed_stream.clone(), vec![count]){
                        Ok(_)=>{},
                        Err(e)=>{log::error!("Failed Gossiping message with Error: {:?}",e)}
                        }
            },
            event = guard.select_next_some() =>{
                 match event{
                    SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {:?}", address),
                    SwarmEvent::Behaviour(GossipsubEvent::Message { propagation_source:_, message_id:_, message }) => 
                    {
                        let source= message.source.unwrap().to_base58();
                        let data :&u8= message.data.get(0).unwrap();
                        log::info!("peer:{:?} sent {:?}",source,data);
                        //why vec![peer_id] wont add the element in or_insert?
                        if count_map.entry(*data).or_insert(Vec::new()).contains(&peer_id)==false{
                            count_map.get_mut(data).unwrap().push(peer_id.clone());
                            match guard.behaviour_mut().publish(witnessed_stream.clone(), vec![count]){
                                     Ok(_)=>{},
                                     Err(e)=>{log::error!("Failed Gossiping message with Error: {:?}",e)}
                                }
                        }
                        if count_map.get(data).unwrap().contains(&source)==false{
                            count_map.get_mut(data).unwrap().push(source);
                            if count_map.get_mut(data).unwrap().len() == 4{
                                log::info!("received all gossiped messages of count {}",count);
                                count+=1;
                                if count == 5 {
                                    drop(guard);
                                    break
                                };
                            }
                        }
                    },
                    SwarmEvent::Behaviour(GossipsubEvent::Subscribed { peer_id:_, topic:_ }) => {}
                    _ => {},
                    }
                }
            }
        }
        tokio::spawn(async move{
                StreamsGossip::handle_incoming_messages(swarm_clone).await;
        });
        loop{
                streams_gossip.publish(witnessed_stream.clone(), vec![5,6,7]).await;
                tokio::time::sleep(Duration::from_millis(5000)).await;
        }
    }
}
