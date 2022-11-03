use subxt::tx::SubmittableExtrinsic;
pub use tonic::{transport::Server, Request, Response, Status};
use validated_streams::{Stream,StreamStatus,WitnessedStream};
use validated_streams::streams_server::{Streams, StreamsServer};
use subxt::{OnlineClient, PolkadotConfig};

#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams
{
    tonic::include_proto!("validated_streams");
}
#[derive(Default)]
pub struct MyStreams{}

#[tonic::async_trait]
impl Streams for MyStreams
{
    async fn validate_stream(&self,request:Request<Stream>) -> Result<Response<StreamStatus>,Status>
    {
        log::info!("Received a request from {:?}",request.remote_addr());
        let stream =  request.into_inner();
        let api = OnlineClient::<PolkadotConfig>::new().await.expect("failed creating client");
        let submitable_stream = SubmittableExtrinsic::from_bytes(api, stream.extrinsic);
        submitable_stream.submit().await.expect("failed submitting extrinsic");
        let reply = StreamStatus 
        {
            status: String::from("Stream Submitted for validation"),
        };
        Ok(Response::new(reply))
    } 
    async fn witnessed(&self,request:Request<WitnessedStream>) -> Result<Response<StreamStatus>,Status>
    {
        log::info!("Received a request from {:?}",request.remote_addr());
        let reply = StreamStatus 
        {
            status: String::from("Stream Witnessed"),
        };
        Ok(Response::new(reply))
    }
}
impl MyStreams {
    //could prossibly make use of node configs in the future from runner in command.rs
    #[tokio::main]
    pub async fn run() 
    {
        let addr = "127.0.0.1:5555".parse().expect("Can't parse Address into SocketAddr");
        let streams = MyStreams::default();
        println!("Streams server listening on {}", addr);
        match tokio::spawn(async move{
                Server::builder().add_service(StreamsServer::new(streams)).serve(addr).await
        }).await
        {
            Ok(_) => (),
            Err(e) => {panic!("Failed Creating StreamsServer due to Err: {}",e); }
        }
    }
}
