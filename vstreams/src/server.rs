//! A GRPC server for submitting event hashes from a trusted client.

use crate::{
	errors::Error,
	traits::{EventValidatorTrait, EventWitnesserTrait},
};
use futures::{future, stream, Stream};
use sp_core::H256;
use std::{net::SocketAddr, pin::Pin, sync::Arc};
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

/// Run a GRPC server with the ValidatedStreamsGrpc service.
pub async fn run<
	EventWitnesser: EventWitnesserTrait + Sync + Send + 'static,
	EventValidator: EventValidatorTrait + Sync + Send + 'static,
>(
	event_witnesser: Arc<EventWitnesser>,
	event_validator: Arc<EventValidator>,
	grpc_addrs: Vec<SocketAddr>,
) -> Result<(), Error> {
	log::info!(
		"GRPC server can be reached at {}",
		grpc_addrs.iter().fold(String::new(), |acc, &arg| format!("{acc}, {arg}"))
	);

	future::try_join_all(grpc_addrs.into_iter().map(|a| {
		Server::builder()
			.add_service(StreamsServer::new(ValidatedStreamsGrpc {
				event_witnesser: event_witnesser.clone(),
				event_validator: event_validator.clone(),
			}))
			.serve(a)
	}))
	.await
	.map_err(|e| Error::Other(e.to_string()))?;

	Ok(())
}

/// Implements a GRPC server for submitting event hashes from the trusted client.
/// See <https://github.com/comrade-coop/validated-streams/blob/master/proto/streams.proto>) for the protobuf file and associated documentation.
pub struct ValidatedStreamsGrpc<EventWitnesser, EventValidator> {
	/// A [EventWitnesserTrait] instance.
	pub event_witnesser: Arc<EventWitnesser>,
	/// A [EventValidatorTrait] instance.
	pub event_validator: Arc<EventValidator>,
}
#[tonic::async_trait]
impl<
		EventWitnesser: EventWitnesserTrait + Sync + Send + 'static,
		EventValidator: EventValidatorTrait + Sync + Send + 'static,
	> Streams for ValidatedStreamsGrpc<EventWitnesser, EventValidator>
{
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

		self.event_witnesser
			.witness_event(event_id)
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
		let request = request.into_inner();

		let mut from_block = request.from_block;
		if from_block == 0 && request.from_latest {
			from_block =
				self.event_validator.get_latest_finalized_block().await.unwrap_or_default();
		}

		Ok(Response::new(Box::pin(stream::unfold(
			// We pass the event_validator as "state", because it's an Arc<> and it doesn't have
			// Copy to move it in the FnMut
			(self.event_validator.clone(), from_block),
			async move |(event_validator, block_num)| {
				let next_block = block_num + 1;

				let events = match event_validator.get_finalized_block_events(block_num).await {
					Err(e) =>
						return Some((
							Err(Status::aborted(e.to_string())),
							(event_validator, next_block),
						)),
					Ok(events) => events,
				};

				let events = events
					.into_iter()
					.map(|event_id| ValidatedEvent { event_id: event_id.as_ref().to_vec() })
					.collect();

				Some((
					Ok(ValidatedEventsResponse { next_block, events }),
					(event_validator, next_block),
				))
			},
		))))
	}
}
