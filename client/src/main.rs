use sp_keyring::AccountKeyring;
use subxt::{tx::{PairSigner,BaseExtrinsicParamsBuilder}, OnlineClient, PolkadotConfig};
use validated_streams::Stream;
use validated_streams::streams_client::StreamsClient;
pub use tonic::Request;
use std::str;
#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams
{
    tonic::include_proto!("validated_streams");
}

pub async fn create_signed_stream() -> Result<Stream, Box<dyn std::error::Error>>
{
        //for some reason type neeed to explicitly specefied?
        let signer:PairSigner<PolkadotConfig,sp_keyring::sr25519::sr25519::Pair>=PairSigner::new(AccountKeyring::Alice.pair());
        let api = OnlineClient::<PolkadotConfig>::new().await?;
        let stream_id = subxt::ext::sp_core::H256::repeat_byte(0);
        let tx = stream_node::tx()
            .validated_streams()
            .validate_stream(stream_id);

        let submitable_extrinsic = api.tx().create_signed(&tx, &signer,BaseExtrinsicParamsBuilder::new()).await?;
        let encoded_extrinsic = submitable_extrinsic.encoded();
        let stringifed_id = str::from_utf8(&stream_id.0).expect("failed to stringify the id");
        Ok(Stream{
                stream_id: stringifed_id.to_string(),
                extrinsic: encoded_extrinsic.to_vec()
            })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    println!("Attempting to connect");
    let mut client = StreamsClient::connect("http://127.0.0.1:5555").await?;
    let stream = create_signed_stream().await?;
    let request = Request::new(stream);
    let response = client.validate_stream(request).await?;
    println!("Reply received from server {:?}",response);
    Ok(())
}
