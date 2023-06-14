use futures::{stream, StreamExt};
use sha2::{Digest, Sha256};
use std::{
	env,
	io::Write,
	time::{Duration, Instant},
};
use tonic::{transport::Channel, Request};

mod validated_streams {
	tonic::include_proto!("validated_streams");
}
use validated_streams::{
	streams_client::StreamsClient, ValidatedEventsRequest, WitnessEventRequest,
};

const CONCURRENT_REQUESTS: usize = 8;

fn event_num_to_event_id(event_num: u32) -> Vec<u8> {
	let num_bytes = event_num.to_be_bytes();
	let mut event_id = Sha256::digest(num_bytes);
	event_id.as_mut_slice().write_all(&num_bytes).unwrap();
	event_id.to_vec()
}

fn event_id_to_event_num(event_id: &[u8]) -> u32 {
	let num_bytes = &event_id[0..4];
	u32::from_be_bytes(num_bytes.try_into().unwrap())
}

async fn wait_validators(mut client: StreamsClient<Channel>) {
	let request = WitnessEventRequest { event_id: event_num_to_event_id(0) };
	loop {
		let request = Request::new(request.clone());
		if client.witness_event(request).await.is_err() {
			println!("Target node is not a validator yet");
			tokio::time::sleep(Duration::from_secs(5)).await;
		} else {
			break
		}
	}
}
async fn send_events(client: StreamsClient<Channel>, from_num: u32, to_num: u32) {
	let mut events = Vec::new();
	for i in from_num + 1..to_num + 1 {
		events.push(WitnessEventRequest { event_id: event_num_to_event_id(i) });
	}
	stream::iter(events)
		.map(|event| {
			let mut client = client.clone();
			tokio::spawn(async move {
				let request = Request::new(event.clone());
				client.witness_event(request).await
			})
		})
		.buffer_unordered(CONCURRENT_REQUESTS)
		.for_each(|result| async {
			result.unwrap().unwrap();
		})
		.await;
}

async fn witness_events(
	mut client: StreamsClient<Channel>,
	limit: u32,
	increase_factor: f32,
	decrease_factor: f32,
) {
	let request = Request::new(ValidatedEventsRequest { from_block: 0, from_latest: true });
	let mut stream = client.validated_events(request).await.unwrap().into_inner();
	let mut max_block_events = 0;
	let mut received_events: u32 = 0;
	let mut num_blocks = 0;
	let mut max_received_event_id = 0;
	let mut sent_events = 0;
	let mut events_per_block = 1;
	let start_instant = Instant::now();
	while let Some(response) = stream.message().await.unwrap() {
		let current_block_events = response.events.len() as u32;
		if let Some(max_id) =
			response.events.iter().map(|event| event_id_to_event_num(&event.event_id)).max()
		{
			if max_id > max_received_event_id {
				max_received_event_id = max_id;
			}
		}

		num_blocks += 1;
		received_events += current_block_events;
		let current_block_instant = Instant::now();
		let total_time = (current_block_instant - start_instant).as_secs_f32();

		println!("Block: #{num_blocks} finalized");
		println!(
			"Events in block curr / avg #: {current_block_events} / {}",
			received_events as f32 / num_blocks as f32
		);
		println!("Total events sent / received #: {sent_events} / {received_events}");
		println!(
			"Validation throughput (avg / max) s: {} / {}",
			received_events as f32 / total_time,
			max_block_events as f32 * num_blocks as f32 / total_time
		);

		if current_block_events > max_block_events {
			max_block_events = current_block_events;
			println!("\n🚀 NEW PEAK EVENTS IN BLOCK! {max_block_events}\n");
		}

		// We want to spam enough events to fill all the validator's queues, but not so much we
		// outright overwhelm it. sent_events is conceptually the intergal of events_per_block(t)
		// received_events is conceptually the intergal of min(events_per_block(t),
		// processing_limit), for t lagging latency blocks behind Hence, (sent_events -
		// received_events) is roughly equal to events_per_block * latency before we hit the limit;
		// and, once we hit the limit, becomes roughly equal to (events_per_block -
		// processing_limit) * num_blocks + events_per_block * latency Then, (sent_events -
		// received_events) / num_blocks should slowly approach zero before we hit the limit;
		// and, once we hit the limit, will start rising up at a rate of (events_per_block -
		// processing_limit)
		if sent_events < limit {
			let average_unprocessed_events =
				(sent_events - received_events) as f32 / num_blocks as f32;

			if average_unprocessed_events <= events_per_block as f32 {
				events_per_block = (events_per_block as f32 * increase_factor) as u32;
			} else if events_per_block > 1 {
				events_per_block = (events_per_block as f32 / decrease_factor) as u32;
			}

			let mut new_sent_events = sent_events + events_per_block;
			if new_sent_events >= limit {
				new_sent_events = limit;
				println!("\nReached limit, waiting for all events to be processed...\n");
			} else {
				println!("Sending {events_per_block} more events");
			}
			tokio::spawn(send_events(client.clone(), sent_events, new_sent_events));
			sent_events = new_sent_events;
		}
		if received_events >= limit {
			println!("\nReceived all events!\n");
			break
		}
	}

	let total_time = (Instant::now() - start_instant).as_secs_f32();

	println!("Block: #{num_blocks} finalized");
	println!(
		"Events in block avg / max #: {} / {max_block_events}",
		received_events as f32 / num_blocks as f32
	);
	println!("Total events sent / received #: {sent_events} / {received_events}");
	println!(
		"Validation throughput (avg / max) s: {} / {}",
		received_events as f32 / total_time,
		max_block_events as f32 * num_blocks as f32 / total_time
	);
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args: Vec<String> = env::args().collect();
	if args.len() < 4 {
		println!(
			"USAGE: {} <Target_Address> <Increase_Factor> <Decrease_Factor> <Max_events>",
		   args.get(0).unwrap_or(&"vstreams_tps_benchmark".to_string())
		);
		return Ok(())
	}
	let validator_addr = args.get(1).expect("wrong ip address").clone();
	let increase_factor = args.get(2).expect("bad increase factor").clone().parse::<f32>().unwrap();
	let decrease_factor = args.get(3).expect("bad increase factor").clone().parse::<f32>().unwrap();
	let limit = args.get(4).expect("bad max events value").clone().parse::<u32>().unwrap();

	println!("Connecting to {validator_addr}...");

	let client = loop {
		if let Ok(val) = StreamsClient::connect(validator_addr.clone()).await {
			break val;
		} else {
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	};

	// wait until validator is up
	wait_validators(client.clone()).await;
	println!(
		"Connected to {validator_addr}, Increase factor: {increase_factor}, Decrease factor: {decrease_factor}, Max events: {limit}"
	);
	witness_events(client, limit, increase_factor, decrease_factor).await;
	Ok(())
}
