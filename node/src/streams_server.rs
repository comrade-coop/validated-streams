use std::thread;
use std::time::Duration;
use std::io::ErrorKind;
use futures::lock::Mutex;
use sp_runtime::KeyTypeId;
use sp_runtime::app_crypto::CryptoTypePublicPair;
use subxt::tx::SubmittableExtrinsic;
use tonic::transport::{Channel, Endpoint};
pub use tonic::{transport::Server, Request, Response, Status};
use validated_streams::{Stream,StreamStatus,WitnessedStream};
use validated_streams::streams_server::{Streams, StreamsServer};
use subxt::{OnlineClient, PolkadotConfig};
use std::sync::Arc;
use crate::network_configs::NetworkConfiguration;
use crate::stream_proofs::{StreamProofs, InMemoryStreamProofs};
use std::io::Error;
use self::validated_streams::WitnessedStreamReply;
use self::validated_streams::streams_client::StreamsClient;
use sp_keystore::CryptoStore;

#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams
{
    tonic::include_proto!("validated_streams");
}

pub struct ValidatedStreamsNode
{
    target:u16,
    peers : Vec<Endpoint>,
    validators_connections : Arc<Mutex<Vec<StreamsClient<Channel>>>>,
    stream_proofs: Box<dyn StreamProofs + Send + Sync>,
    keystore : Arc<dyn CryptoStore>,
    key_type: KeyTypeId,
    pub_key:CryptoTypePublicPair,
}

#[tonic::async_trait]
impl Streams for ValidatedStreamsNode
{
    //check if the watcher(client) has already submitted the stream
    //if not create a WitnessedStream message, add it to the stream proofs and gossip it
    async fn validate_stream(&self,request:Request<Stream>) -> Result<Response<StreamStatus>,Status>
    {
        if let Some(remote_addr) = request.remote_addr()
        {
            log::info!("Received a request from {:?}",remote_addr);
            let stream = request.into_inner(); 
            let mut reply = StreamStatus 
            {
                status: String::from("Stream Submitted for validation"),
            };
            if self.stream_proofs.contains(stream.stream_id.clone())
            {
                reply.status = String::from("Stream Already submitted");
                Ok(Response::new(reply))
            }else
            {
                match self.create_witnessed_stream(stream).await
                {
                    Ok(witnessed_stream)=>
                    {
                        self.process_witness_result(self.stream_proofs.add_stream_proof(witnessed_stream.clone(),remote_addr.to_string()),witnessed_stream.clone()).await.ok();
                        match ValidatedStreamsNode::gossip(self.validators_connections.clone(), witnessed_stream).await
                        {
                            Ok(_) => Ok(Response::new(reply)),
                            Err(e) => Err(Status::aborted(e.to_string())),
                        }
                    }
                    Err(e)=>
                    {
                        Err(Status::aborted(e.to_string()))
                    }    
                }
            }
        }else{
            Err(Status::aborted("Malformed Request, can't retreive Origin address"))
        }
    } 
    //receive what other validators have witnessed
    async fn witnessed(&self,request:Request<WitnessedStream>) -> Result<Response<WitnessedStreamReply>,Status>
    {
        //check signature, call add_stream_proof
        if let Some(remote_addr) = request.remote_addr()
        {
            let witnessed_stream = request.into_inner();
            if ValidatedStreamsNode::verify_witnessed_stream(witnessed_stream.clone())
            {
                log::info!("Received a request from {:?}",remote_addr);
                self.process_witness_result(self.stream_proofs.add_stream_proof(witnessed_stream.clone(),remote_addr.to_string()),witnessed_stream).await
            }else
            {
                let mut reply = WitnessedStreamReply 
                {
                    reply: String::from(""),
                };
                reply.reply = String::from("INVALID Witnessed Stream Signature");
                Ok(Response::new(reply))
            }
        }else
        {
            Err(Status::aborted("Malformed Request, can't retreive Origin address"))
        }
    }
}
impl ValidatedStreamsNode {
    pub async fn new(peers: Vec<Endpoint>,keystore:Arc<dyn CryptoStore>) -> ValidatedStreamsNode
    {
        let peers_length= peers.len();
        let validators_length = peers_length +1;
        let target = (2*((validators_length-1)/3)+1) as u16 ;
        log::info!("Minimal number of nodes that needs to witness Streams is:{}",target);
        let key_type =sp_core::crypto::key_types::AURA;
        keystore.sr25519_generate_new(key_type, None).await.ok();
        let pub_key = keystore.keys(key_type).await.expect("Failed retreiving public keys from keystore").get(0).expect("Failed unwraping retreived key").clone();
        ValidatedStreamsNode { peers, validators_connections: Arc::new(Mutex::new(Vec::with_capacity(peers_length))) , 
        stream_proofs: Box::new(InMemoryStreamProofs::new()),
        target,keystore,key_type,pub_key}
    }
    pub fn verify_witnessed_stream(stream: WitnessedStream)-> bool
    {
        true
    }
    pub async fn process_witness_result(&self,result: Result<u16,Error>,proof:WitnessedStream) -> Result<Response<WitnessedStreamReply>,Status>
    { 
        match result{
                Ok(count) =>
                {
                    let mut reply = WitnessedStreamReply 
                    {
                        reply: String::from("Stream Witnessed"),
                    };
                    // if count == self.target
                    if count == 1
                    {
                        let extrinsic = proof.stream.expect("failed unwraping stream extrinsic").extrinsic;
                        ValidatedStreamsNode::submit_stream_extrinsic(extrinsic).await;
                        reply.reply = String::from("");
                        Ok(Response::new(reply))
                    }else
                    {
                        reply.reply= String::from("Proof Count increased");
                        Ok(Response::new(reply))
                    }
                }
                Err(e) => { if let Some(e) = e.into_inner(){
                        Err(Status::already_exists(e.to_string()))
                    }else{
                        Err(Status::already_exists("Already Witnessed"))
                    }
            }
        }
    }
    pub async fn submit_stream_extrinsic(extrinsic:Vec<u8>) 
    {
        let api = OnlineClient::<PolkadotConfig>::new().await.expect("failed creating substrate client");
        let submitable_stream = SubmittableExtrinsic::from_bytes(api, extrinsic);
        match submitable_stream.submit().await
        {
            Ok(v) => {
                log::info!("Stream submitted with hash: {}",v);
            }
            Err(e) => {log::info!("Failed submitting stream with Error: {}",e);}
        }
    }
    pub async fn create_witnessed_stream(&self,stream:Stream) -> Result<WitnessedStream,Error>
    {
        match self.keystore.sign_with(self.key_type, &self.pub_key, stream.extrinsic.as_slice()).await
        {
            Ok(v) => {if let Some(sig) = v {
                    log::info!("signature of stream {:?}",sig);
                    Ok(WitnessedStream { signature: sig, stream: Some(stream.clone()) })
                }
                else
                {
                    Err(Error::new(ErrorKind::Other, "Failed retriving signature"))
                }
            }
            Err(e)=>
            {
                Err(Error::new(ErrorKind::Other, "Could not sign Witnessed stream"))
            }
        }
        
        
    }
    pub async fn intialize_mesh_network(&mut self) 
    {
        let connections = self.validators_connections.clone(); 
        let peers = self.peers.clone();
        tokio::spawn(async move
            {
                log::info!("waiting server to get started");
                thread::sleep(Duration::from_millis(4000));
                for addr in peers {
                    let connection_result = StreamsClient::<tonic::transport::Channel>::connect(addr.clone()).await;
                    match connection_result
                    {
                        Ok(conn) =>{
                            log::info!("🤜🤛Connected successfully to validator");
                            connections.lock().await.push(conn);}, 
                        Err(e) => {
                            log::error!("failed connecting to address {:?} with error {:?}",addr,e);
                        }
                    }
            }
        });
    } 
    pub async fn gossip(connections: Arc<Mutex<Vec<StreamsClient<Channel>>>>,stream: WitnessedStream)  -> Result<(),tonic::transport::Error> 
    {
          for conn in &mut connections.lock().await.iter_mut() {
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
    pub async fn run<T>(configs:T,keystore :Arc<dyn CryptoStore>) where T:NetworkConfiguration 
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
        let mut streams = ValidatedStreamsNode::new(target,keystore).await;
        streams.intialize_mesh_network().await;
        match tokio::spawn(async move{
            log::info!("Streams server listening on [::0]:5555]");
            Server::builder().add_service(StreamsServer::new(streams)).serve("[::0]:5555".parse().expect("Failed parsing gRPC server Address")).await
        }).await
        {
            Ok(_) => (),
            Err(e) => {panic!("Failed Creating StreamsServer due to Err: {}",e); }
        }
    }
}
