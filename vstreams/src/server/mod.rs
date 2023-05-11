//! A GRPC server for submitting event hashes from a trusted client.

use crate::{configs::FullClient, services::events::EventService};
use local_ip_address::local_ip;
use std::pin::Pin;

use node_runtime::pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockBackend, BlockchainEvents};
use sp_api::ProvideRuntimeApi;
use sp_core::H256;

use futures::{stream, Stream, StreamExt};
use sp_blockchain::lowest_common_ancestor;
use std::{
	io::{Error, ErrorKind},
	sync::Arc,
};
use tonic::{transport::Server, Request, Response, Status};
use validated_streams_proto::{
	streams_server::{Streams, StreamsServer},
	ValidatedEvent, ValidatedEventsRequest, ValidatedEventsResponse, WitnessEventRequest,
	WitnessEventResponse,
};

/// The GRPC/protobuf module implemented by the GRPC server
pub mod validated_streams_proto {
	#![allow(missing_docs)]
	tonic::include_proto!("validated_streams");
}

/// Implements a GRPC server for submitting event hashes from the trusted client.
/// See <https://github.com/comrade-coop/validated-streams/blob/master/proto/streams.proto>) for the protobuf file and associated documentation.
pub struct ValidatedStreamsGrpc {
	events_service: Arc<EventService>,
	client: Arc<FullClient>,
}
impl ValidatedStreamsGrpc {
	/// Run the GRPC server.
	pub async fn run(
		client: Arc<FullClient>,
		events_service: Arc<EventService>,
		grpc_port: u16,
	) -> Result<(), Error> {
		log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
		Server::builder()
			.add_service(StreamsServer::new(ValidatedStreamsGrpc { events_service, client }))
			.serve(
				format!("[::0]:{}", grpc_port)
					.parse()
					.expect("Failed parsing gRPC server Address"),
			)
			.await
			.map_err(|e| Error::new(ErrorKind::Other, e.to_string()))
	}
}

#[tonic::async_trait]
impl Streams for ValidatedStreamsGrpc {
	async fn witness_event(
		&self,
		request: Request<WitnessEventRequest>,
	) -> Result<Response<WitnessEventResponse>, Status> {
		let event = request.into_inner();
		let event_id = if event.event_id.len() == 32 {
			Ok(H256::from_slice(event.event_id.as_slice()))
		} else {
			Err(Status::invalid_argument("invalid event_id length (expected 32 bytes)"))
		}?;

		self.events_service
			.handle_client_request(event_id)
			.await
			.map_err(|e| Status::aborted(e.to_string()))?;

		Ok(Response::new(WitnessEventResponse {}))
	}

	// This type looks terrifying, but I'm blaming tonic; even their examples have that!
	type ValidatedEventsStream =
		Pin<Box<dyn Stream<Item = Result<ValidatedEventsResponse, Status>> + Send>>;

	async fn validated_events(
		&self,
		request: Request<ValidatedEventsRequest>,
	) -> Result<Response<Self::ValidatedEventsStream>, Status> {
		Ok(Response::new(Box::pin(stream::unfold(
			// We pass the client as "state", because it's an Arc<> and it doesn't have Copy to
			// move it in the FnMut
			(self.client.clone(), request.into_inner().from_block),
			async move |(client, block_num)| {
				let mut last_finalized = client.chain_info().finalized_hash;

				let block_id = loop {
					if let Ok(Some(block_hash)) = client.block_hash(block_num) {
						// If the block at block_num is part of the chain...
						if let Ok(common_ancestor) =
							lowest_common_ancestor(client.as_ref(), last_finalized, block_hash)
						{
							if common_ancestor.hash == block_hash {
								// ...And is part of the finalized chain (LCA between it and the
								// finalized tip is the block itself)
								break block_hash // Then, the block at block_num id was finalized
							}
						}
					}
					// Otherwise, wait for the next change of the finalized chain and try again
					last_finalized =
						client.finality_notification_stream().select_next_some().await.hash;
				};

				let block_extrinsics =
					client.block_body(block_id.clone()).ok().flatten().unwrap_or_default();
				let event_ids = client
					.runtime_api()
					.get_extrinsic_ids(block_id, &block_extrinsics)
					.unwrap_or_default();

				let result = Ok(ValidatedEventsResponse {
					next_block: block_num + 1,
					events: event_ids
						.iter()
						.map(|id| ValidatedEvent { event_id: id.as_ref().to_vec() })
						.collect(),
				});

				Some((result, (client, block_num + 1)))
			},
		))))
	}
}
