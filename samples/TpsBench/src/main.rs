pub mod validated_streams {
	tonic::include_proto!("validated_streams");
}
use std::{env, time::Duration};

pub use tonic::Request;
use sha2::{Sha256, Digest};
pub const INIT_NUM : u32 = 25;
use validated_streams::{streams_client::StreamsClient, WitnessEventRequest, ValidatedEventsRequest};
async fn wait_validators(validator_addr: String) {
    let mut client = StreamsClient::connect(validator_addr.clone()).await.unwrap();
    let event : u32 = 0;
    let hash_bytes = Sha256::digest(&event.to_be_bytes());
    let request = WitnessEventRequest{event_id:hash_bytes.to_vec()};
    loop{
    let request = Request::new(request.clone());
        if client.witness_event(request).await.is_err(){
            println!("Target node:{} is not a validator yet",validator_addr);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }else{
            break;
        }
    }
}
async fn send_events(from_num: u32, to_num: u32, validator_addr: String) {
    let mut client = StreamsClient::connect(validator_addr).await.unwrap();
    let mut events = Vec::new();
    for i in from_num+1..to_num+1{
		let hash_bytes = Sha256::digest(&i.to_be_bytes());
        events.push(WitnessEventRequest{event_id:hash_bytes.to_vec()});
    }
    for event in events{
        let request = Request::new(event.clone());
        client.witness_event(request).await.unwrap();
    }
}

async fn witness_events(validator_addr: String,limit : u32, increase_factor:u32) {
	let mut client = StreamsClient::connect(validator_addr.clone()).await.unwrap();
	let request = Request::new(ValidatedEventsRequest{from_block:0, from_latest:true});
	let mut stream = client.validated_events(request).await.unwrap().into_inner();
	let mut max_block_txs = 0;
	let mut total_events:u32 = 0;
	let mut num_blocks = 0;
	let mut num_events = INIT_NUM;
	let mut sent_events = INIT_NUM ;
	let mut average_events_per_block = 0;
	while let Some(response) = stream.message().await.unwrap() {
		let current_block_txs = response.events.len() as u32;
		num_blocks+=1;
		let new_average = total_events / num_blocks;
		total_events += current_block_txs;
		println!("\nCurrent Block contains {} events",current_block_txs);
		println!("Block:#{} finalized",num_blocks);
		println!("Total number of validated events: {}",total_events);
		println!("📊 Average number of events per Block: {}\n",new_average);
		if current_block_txs > max_block_txs {
			max_block_txs = current_block_txs;
			println!("\n🚀 NEW Max TXS in Block:{}", max_block_txs);
			println!("🌟 NEW Max TPS :{} \n", max_block_txs/6);
		}
		// if the average number of events drop and the total number of events reached the number of the last batch
		// send new batch of txs to make sure we have more events in the tx pool than
		// what the block can contain, this will be a balnaced approach of witnessing lots of events and
		// also let the validator focus more on processing gossiped witnessed events from his peers
		else if  (new_average < average_events_per_block) && (new_average != 0)  && (total_events >=  sent_events) && (num_events <= limit) {
			let new_num_events = num_events * increase_factor;
			println!("\n🩸 Average number of events dropped, sending {} events", new_num_events - num_events);
			let validator_addr_clone = validator_addr.clone();
			tokio::spawn(async move{send_events(num_events, new_num_events,validator_addr_clone.clone()).await});
			num_events = new_num_events;
			sent_events = new_num_events +1 ;
			println!("📩 Total number of events sent {}\n",sent_events);
		}
		if num_events >= limit {
			println!("\nMax TXS in Block:{}", max_block_txs);
			println!("Max TPS :{} \n", max_block_txs/6);
		}
		average_events_per_block = new_average;
	}
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args: Vec<String> = env::args().collect();
	if args.len() < 4 {
		println!("USAGE: tps_bench <Target_Address> <Increase_Factor> <Max_events>");
		return Ok(())
	}
	let validator_addr = args.get(1).expect("wrong ip address").clone();
	let increase_factor = args.get(2).expect("bad increase factor").clone().parse::<u32>().unwrap();
	let limit = args.get(3).expect("bad max events value").clone().parse::<u32>().unwrap();

	// wait until validator is up
	wait_validators(validator_addr.clone()).await;
	println!("Connected to {}, Increase factor:{}, Max events:{}",validator_addr, increase_factor, limit);
	send_events(0,INIT_NUM,validator_addr.clone()).await;
	let _ = tokio::spawn(async move{witness_events(validator_addr,limit,increase_factor).await;}).await;
	Ok(())
}
