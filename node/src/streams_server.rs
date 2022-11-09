use std::thread;
use std::time::Duration;
use subxt::tx::SubmittableExtrinsic;
use tonic::transport::{Channel, Endpoint};
pub use tonic::{transport::Server, Request, Response, Status};
use validated_streams::{Stream,StreamStatus,WitnessedStream};
use validated_streams::streams_server::{Streams, StreamsServer};
use subxt::{OnlineClient, PolkadotConfig};
use std::sync::{Arc, Mutex};
use crate::network_configs::NetworkConfiguration;

use self::validated_streams::WitnessedStreamReply;
use self::validated_streams::streams_client::StreamsClient;

#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams
{
    tonic::include_proto!("validated_streams");
}
#[derive(Default)]
pub struct ValidatedStreamsNode
{
    peers : Vec<Endpoint>,
    validators_connections : Arc<Mutex<Vec<StreamsClient<Channel>>>>,
}

#[tonic::async_trait]
impl Streams for ValidatedStreamsNode
{
    async fn validate_stream(&self,request:Request<Stream>) -> Result<Response<StreamStatus>,Status>
    {
        log::info!("Received a request from {:?}",request.remote_addr());
        let stream =  request.into_inner();
        let api = OnlineClient::<PolkadotConfig>::new().await.expect("failed substrate creating client");
        let submitable_stream = SubmittableExtrinsic::from_bytes(api, stream.extrinsic);
        submitable_stream.submit().await.expect("failed submitting extrinsic");
        let reply = StreamStatus 
        {
            status: String::from("Stream Submitted for validation"),
        };
        Ok(Response::new(reply))
    } 
    async fn witnessed(&self,request:Request<WitnessedStream>) -> Result<Response<WitnessedStreamReply>,Status>
    {
        log::info!("Received a request from {:?}",request.remote_addr());
        let reply = WitnessedStreamReply 
        {
            reply: String::from("Stream Witnessed"),
        };
        Ok(Response::new(reply))
    }
}
impl ValidatedStreamsNode {
    pub fn new(peers: Vec<Endpoint>) -> ValidatedStreamsNode
    {
        let peers_length= peers.len();
        ValidatedStreamsNode { peers, validators_connections: Arc::new(Mutex::new(Vec::with_capacity(peers_length))) }
    }
    pub async fn intialize_mesh_network(&mut self) 
    {
        let connections = self.validators_connections.clone(); 
        let peers = self.peers.clone();
        tokio::spawn(async move
            {
                log::info!("waiting server to get started");
                thread::sleep(Duration::from_millis(2000));
                log::info!("slept");
                for addr in peers {
                    let connection_result = StreamsClient::<tonic::transport::Channel>::connect(addr.clone()).await;
                    match connection_result
                    {
                        Ok(conn) =>{
                            log::info!("🤜🤛Connected successfully to validator with addr: {:?}",addr.clone());
                            connections.lock().unwrap().push(conn);}, 
                        Err(e) => {
                            log::error!("failed connecting to address {:?} with error {:?}",addr,e);
                        }
                    }
            }
        });
    } 
    pub async fn gossip(&mut self,stream: WitnessedStream)  -> Result<(),tonic::transport::Error> 
    {
          for conn in &mut self.validators_connections.lock().unwrap().iter_mut() {
            let reply = conn.witnessed(Request::new(stream.clone())).await;
            match reply
            {
                Ok(client_reply) => log::info!("{:?}",client_reply),
                Err(e) => {
                    log::info!("failed sending witnessing stream with err {:?}",e);
                }
             }
          }
          Ok(())
    }
    //could prossibly make use of node configs in the future from runner in command.rs
    pub async fn run<T>(configs:T) where T:NetworkConfiguration 
    {
        let addr = configs.get_self_address();
        let self_endpoint = format!("http://{}",addr);
        let peers = configs.get_peers_addresses();
        let mut target :Vec<Endpoint> = Vec::new();
        for peer in peers.iter()
        {
            if *peer != self_endpoint
            {
                target.push(peer.clone().parse().expect("invalid Endpoint"));
            }
        }
        let mut streams = ValidatedStreamsNode::new(target);
        streams.intialize_mesh_network().await;
        match tokio::spawn(async move{
            log::info!("Streams server listening on [::0]:5555]");
            Server::builder().add_service(StreamsServer::new(streams)).serve("[::0]:5555".parse().unwrap()).await
        }).await
        {
            Ok(_) => (),
            Err(e) => {panic!("Failed Creating StreamsServer due to Err: {}",e); }
        }
    }
}
