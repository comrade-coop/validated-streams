//! A GRPC server for submitting event hashes from a trusted client.

use crate::{chain_info::ChainInfo, events::EventWitnessHandler};
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

/// Run a GRPC server with the ValidatedStreamsGrpc service.
pub async fn run<EventWitness: EventWitnessHandler + Sync + Send + 'static, Block: BlockT, Client>(
	client: Arc<Client>,
	event_witness: Arc<EventWitness>,
	grpc_port: u16,
) -> Result<(), Error>
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
	log::info!("GRPC server can be reached at 0.0.0.0:{grpc_port}");
	Server::builder()
		.add_service(StreamsServer::new(ValidatedStreamsGrpc {
			event_witness,
			event_validator: Arc::new(EventValidator::<Client, Block>::new(client)),
		}))
		.serve(
			format!("[::0]:{grpc_port}")
				.parse()
				.expect("Failed parsing gRPC server Address"),
		)
		.await
		.map_err(|e| Error::new(ErrorKind::Other, e.to_string()))
}

/// Implements a GRPC server for submitting event hashes from the trusted client.
/// See <https://github.com/comrade-coop/validated-streams/blob/master/proto/streams.proto>) for the protobuf file and associated documentation.
pub struct ValidatedStreamsGrpc<EventWitness, EventValidator> {
	/// A [EventWitness] instance.
	pub event_witness: Arc<EventWitness>,
	/// A [EventValidator] instance.
	pub event_validator: Arc<EventValidator>,
}
#[tonic::async_trait]
impl<
		EventWitness: EventWitnessHandler + Sync + Send + 'static,
		EventValidator: EventValidatorTrait + Sync + Send + 'static,
	> Streams for ValidatedStreamsGrpc<EventWitness, EventValidator>
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

/// A trait responsible for getting the stream of validated events
#[async_trait::async_trait]
pub trait EventValidatorTrait {
	/// Get the list of events in a specific block.
	async fn get_finalized_block_events(&self, block_num: u32) -> Result<Vec<H256>, Error>;

	/// Get the latest block.
	async fn get_latest_finalized_block(&self) -> Result<u32, Error>;
}

/// The default implementation of [EventValidatorTrait].
pub struct EventValidator<Client, Block> {
	client: Arc<Client>,
	phantom: PhantomData<Block>,
}

impl<Client, Block> EventValidator<Client, Block> {
	/// Create a new EventValidator for a specific client.
	/// Note that you might need to call this like `EventValidator::<Client, Block>::new(..)`
	/// because of generics
	pub fn new(client: Arc<Client>) -> Self {
		Self { client, phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Client, Block: BlockT> EventValidatorTrait for EventValidator<Client, Block>
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
	async fn get_finalized_block_events(&self, block_num: u32) -> Result<Vec<H256>, Error> {
		let mut last_finalized = self.client.chain_info().finalized_hash;

		let block_id = loop {
			if let Ok(Some(block_hash)) = self.client.block_hash(block_num.into()) {
				// If the block at block_num is part of the chain...
				if let Ok(common_ancestor) =
					lowest_common_ancestor(self.client.as_ref(), last_finalized, block_hash)
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
				self.client.finality_notification_stream().select_next_some().await.hash;
		};

		let block_extrinsics = self.client.block_body(block_id).ok().flatten().unwrap_or_default();

		Ok(self
			.client
			.runtime_api()
			.get_extrinsic_ids(block_id, &block_extrinsics)
			.unwrap_or_default())
	}

	async fn get_latest_finalized_block(&self) -> Result<u32, Error> {
		Ok(self.client.chain_info().finalized_number.into())
	}
}
