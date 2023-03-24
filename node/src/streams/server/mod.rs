//! A GRPC server for submitting event hashes from a trusted client.

use crate::streams::services::events::EventService;
use local_ip_address::local_ip;

use sp_core::H256;

use std::{
	io::{Error, ErrorKind},
	sync::Arc,
};
use tonic::{transport::Server, Request, Response, Status};
use validated_streams_proto::{
	streams_server::{Streams, StreamsServer},
	ValidateEventRequest, ValidateEventResponse,
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
}
impl ValidatedStreamsGrpc {
	/// Run the GRPC server.
	pub async fn run(events_service: Arc<EventService>, grpc_port: u16) -> Result<(), Error> {
		log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
		Server::builder()
			.add_service(StreamsServer::new(ValidatedStreamsGrpc { events_service }))
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
	async fn validate_event(
		&self,
		request: Request<ValidateEventRequest>,
	) -> Result<Response<ValidateEventResponse>, Status> {
		let remote_addr = request
			.remote_addr()
			.ok_or_else(|| Status::aborted("Malformed Request, can't retrieve Origin address"))?;
		log::info!("Received a request from {:?}", remote_addr);
		let event = request.into_inner();
		// check that event_id is 32 bytes otherwise H256::from_slice would panic
		if event.event_id.len() == 32 {
			Ok(Response::new(ValidateEventResponse {
				status: self
					.events_service
					.handle_client_request(H256::from_slice(event.event_id.as_slice()))
					.await
					.map_err(|e| Status::aborted(e.to_string()))?,
			}))
		} else {
			Err(Error::new(ErrorKind::Other, "invalid event_id sent".to_string()).into())
		}
	}
}
