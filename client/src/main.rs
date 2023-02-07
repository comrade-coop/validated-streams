use sp_keyring::AccountKeyring;
use std::{
    time::Duration
};
use subxt::{
	tx::{BaseExtrinsicParamsBuilder, PairSigner},
	OnlineClient, PolkadotConfig,
};
pub use tonic::Request;
use validated_streams::{streams_client::StreamsClient, ValidateEventRequest};
#[subxt::subxt(runtime_metadata_path = "../artifacts/metadata.scale")]
pub mod stream_node {}

pub mod validated_streams {
	tonic::include_proto!("validated_streams");
}

pub async fn create_signed_event() -> Result<ValidateEventRequest, Box<dyn std::error::Error>> {
    let signer: PairSigner<PolkadotConfig, sp_keyring::sr25519::sr25519::Pair> =PairSigner::new(AccountKeyring::Alice.pair());
	let api = OnlineClient::<PolkadotConfig>::new().await?;
	let event_id = subxt::ext::sp_core::H256::repeat_byte(0);
	let tx = stream_node::tx().validated_streams().validate_event(event_id);
	let submitable_extrinsic = api.tx().create_unsigned(&tx)?;
	let encoded_extrinsic = submitable_extrinsic.encoded();
	let stringifed_id = event_id.to_string();
	Ok(ValidateEventRequest {
		event_id: stringifed_id.to_string(),
		extrinsic: encoded_extrinsic.to_vec(),
	})
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let event = create_signed_event().await?;
    let validators:Vec<String>= vec![
        "http://127.0.0.1:5556".to_string(),
        "http://127.0.0.1:5557".to_string(),
        "http://127.0.0.1:5558".to_string(),
        "http://127.0.0.1:5559".to_string(),
    ];
        for validator in validators.into_iter(){
            let mut client = StreamsClient::connect(validator).await?;
            let request = Request::new(event.clone());
            let response = client.validate_event(request).await?;
            println!("Reply received from server {:?}", response);
            tokio::time::sleep(Duration::from_millis(3000)).await;
        }
	Ok(())
}
