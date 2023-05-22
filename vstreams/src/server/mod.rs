//! A GRPC server for submitting event hashes from a trusted client.

use crate::{chain_info::ChainInfo, services::events::EventWitnessHandler};
use local_ip_address::local_ip;
use std::pin::Pin;

use futures::{stream, Stream, StreamExt};
use pallet_validated_streams::ExtrinsicDetails;
use sc_client_api::{BlockBackend, BlockchainEvents};
use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_blockchain::{lowest_common_ancestor, HeaderMetadata};
use sp_core::H256;

use std::{
	io::{Error, ErrorKind},
	marker::PhantomData,
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
pub struct ValidatedStreamsGrpc<EventWitness, Block: BlockT, Client> {
	event_witness: Arc<EventWitness>,
	client: Arc<Client>,
	phantom: PhantomData<Block>,
}
impl<EventWitness: EventWitnessHandler + Sync + Send + 'static, Block: BlockT, Client>
	ValidatedStreamsGrpc<EventWitness, Block, Client>
where
	Client: ChainInfo<Block>
		+ HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>
		+ Sync
		+ Send
		+ 'static,
	Client::Api: ExtrinsicDetails<Block>,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
{
	/// Run the GRPC server.
	pub async fn run(
		client: Arc<Client>,
		event_witness: Arc<EventWitness>,
		grpc_port: u16,
	) -> Result<(), Error> {
		log::info!("Server could be reached at {}", local_ip().unwrap().to_string());
		Server::builder()
			.add_service(StreamsServer::new(ValidatedStreamsGrpc {
				event_witness,
				client,
				phantom: PhantomData,
			}))
			.serve(
				format!("[::0]:{grpc_port}")
					.parse()
					.expect("Failed parsing gRPC server Address"),
			)
			.await
			.map_err(|e| Error::new(ErrorKind::Other, e.to_string()))
	}
}

#[tonic::async_trait]
impl<EventWitness: EventWitnessHandler + Sync + Send + 'static, Block: BlockT, Client> Streams
	for ValidatedStreamsGrpc<EventWitness, Block, Client>
where
	Client: ChainInfo<Block>
		+ HeaderMetadata<Block>
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>
		+ Sync
		+ Send
		+ 'static,
	Client::Api: ExtrinsicDetails<Block>,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u32>,
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

		self.event_witness
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
		Ok(Response::new(Box::pin(stream::unfold(
			// We pass the client as "state", because it's an Arc<> and it doesn't have Copy to
			// move it in the FnMut
			(self.client.clone(), request.from_block),
			async move |(client, mut block_num)| {
				let mut last_finalized = client.chain_info().finalized_hash;

				if block_num == 0 && request.from_latest {
					block_num = client.chain_info().finalized_number.into();
				}

				let block_id = loop {
					if let Ok(Some(block_hash)) = client.block_hash(block_num.into()) {
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
					client.block_body(block_id).ok().flatten().unwrap_or_default();
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
