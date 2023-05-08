//! module for custom transaction pools
use crate::configs::FullClient;
use codec::{Decode, Encode};
use futures::FutureExt;
use node_runtime::{opaque::Block, pallet_validated_streams::ExtrinsicDetails};
use sc_client_api::HeaderBackend;
use sc_network::config::TransactionPool;
use sc_service::{
	InPoolTransaction, IntoPoolError, TransactionImport, TransactionImportFuture,
	TransactionPool as ConfigPool,
};
use sc_transaction_pool::{error::Error, BasicPool, FullChainApi, Transaction};
use sc_transaction_pool_api::{
	error::Error as TxPoolError, MaintainedTransactionPool, PoolFuture, TransactionFor, TxHash,
};
use sp_api::{BlockId, ProvideRuntimeApi};
use sp_core::H256;
use sp_runtime::{traits::Block as BlockT, OpaqueExtrinsic};
use std::{collections::HashMap, sync::Arc};
/// A transaction pool that wraps BasicPool and rejects all gossiped validate_event transactions
/// from peers
pub struct NetworkTxPool(
	pub Arc<BasicPool<FullChainApi<FullClient, Block>, Block>>,
	pub Arc<FullClient>,
);
impl TransactionPool<H256, Block> for NetworkTxPool {
	fn transactions(&self) -> Vec<(H256, <Block as BlockT>::Extrinsic)> {
		self.0
			.ready()
			.filter(|t| t.is_propagable())
			.map(|t| {
				let hash = *t.hash();
				let ex = t.data().clone();
				(hash, ex)
			})
			.collect()
	}

	fn hash_of(&self, _: &<Block as BlockT>::Extrinsic) -> H256 {
		Default::default()
	}

	fn import(&self, ext: <Block as BlockT>::Extrinsic) -> TransactionImportFuture {
		// reject all imported transaction if any node attempt to gossip them
		let encoded = ext.encode();
		let uxt: OpaqueExtrinsic = match Decode::decode(&mut &encoded[..]) {
			Ok(uxt) => uxt,
			Err(e) => {
				log::error!("Transaction invalid: {:?}", e);
				return Box::pin(futures::future::ready(TransactionImport::Bad))
			},
		};

		let best_block_id = BlockId::hash(self.1.info().best_hash);
		let Ok(is_validated_streams) =
			self.1.runtime_api().is_witnessed_event_extrinsic(&best_block_id, uxt.clone())
			else { return async { TransactionImport::None }.boxed() };
		if is_validated_streams {
			log::error!("peer attempted to corrupt the tx pool");
			return async { TransactionImport::Bad }.boxed()
		}

		let pool = self.0.pool().clone();
		Box::pin(async move {
			let import_future = pool.submit_one(
				&best_block_id,
				sc_transaction_pool_api::TransactionSource::External,
				uxt.clone(),
			);
			match import_future.await {
				Ok(_) => TransactionImport::NewGood,
				Err(e) => match e.into_pool_error() {
					Ok(sc_transaction_pool_api::error::Error::AlreadyImported(_)) =>
						TransactionImport::KnownGood,
					Ok(e) => {
						log::error!("Error adding transaction to the pool: {:?}", e);
						TransactionImport::Bad
					},
					Err(e) => {
						log::error!("Error converting pool error: {}", e);
						// it is not bad at least, just some internal node logic error, so peer is
						// innocent.
						TransactionImport::KnownGood
					},
				},
			}
		})
	}
	fn on_broadcasted(&self, propagations: HashMap<H256, Vec<String>>) {
		self.0.on_broadcasted(propagations)
	}

	fn transaction(&self, hash: &H256) -> Option<<Block as BlockT>::Extrinsic> {
		self.0.ready_transaction(hash).and_then(
			// Only propagable transactions should be resolved for network service.
			|tx| if tx.is_propagable() { Some(tx.data().clone()) } else { None },
		)
	}
}
impl NetworkTxPool {
	fn check_extrinsics(
		client: Arc<FullClient>,
		xts: &Vec<sc_transaction_pool_api::TransactionFor<Self>>,
	) -> PoolFuture<Result<(), Error>, Error> {
		for xt in xts {
			let best_block_id = BlockId::hash(client.info().best_hash);
			match client.runtime_api().is_witnessed_event_extrinsic(&best_block_id, xt.clone()) {
				Ok(valid) =>
					if !valid {
						log::error!("💣 Attempt to inject witnessed events in TxPool detected");
						return async {
							Err(Error::Pool(
								TxPoolError::ImmediatelyDropped.into_pool_error().map_err(
									|_| {
										Error::RuntimeApi("Validate_event extrinsics are not meant to be gossiped directly".to_string())
									},
								)?,
							))
						}
						.boxed()
					},
				Err(e) => return async move { Err(Error::RuntimeApi(e.to_string())) }.boxed(),
			}
		}
		Box::pin(async { Ok(Ok(())) })
	}
}
impl ConfigPool for NetworkTxPool {
	type Block = Block;

	type Hash = H256;

	type InPoolTransaction = Transaction<TxHash<Self>, TransactionFor<Self>>;
	type Error = sc_transaction_pool::error::Error;

	fn submit_at(
		&self,
		at: &sp_api::BlockId<Self::Block>,
		source: sc_transaction_pool_api::TransactionSource,
		xts: Vec<sc_transaction_pool_api::TransactionFor<Self>>,
	) -> sc_transaction_pool_api::PoolFuture<
		Vec<Result<sc_transaction_pool_api::TxHash<Self>, Self::Error>>,
		Self::Error,
	> {
		let client_clone = self.1.clone();
		let pool = self.0.clone();
		let at_clone = *at;
		async move {
			match Self::check_extrinsics(client_clone, &xts).await {
				Ok(_) => pool.submit_at(&at_clone, source, xts).await,
				Err(e) => Err(e),
			}
		}
		.boxed()
	}

	fn submit_one(
		&self,
		at: &sp_api::BlockId<Self::Block>,
		source: sc_transaction_pool_api::TransactionSource,
		xt: sc_transaction_pool_api::TransactionFor<Self>,
	) -> sc_transaction_pool_api::PoolFuture<sc_transaction_pool_api::TxHash<Self>, Self::Error> {
		let client_clone = self.1.clone();
		let pool = self.0.clone();
		let at_clone = *at;
		async move {
			match Self::check_extrinsics(client_clone, &vec![xt.clone()]).await {
				Ok(_) => pool.submit_one(&at_clone, source, xt).await,
				Err(e) => Err(e),
			}
		}
		.boxed()
	}

	fn submit_and_watch(
		&self,
		at: &sp_api::BlockId<Self::Block>,
		source: sc_transaction_pool_api::TransactionSource,
		xt: sc_transaction_pool_api::TransactionFor<Self>,
	) -> sc_transaction_pool_api::PoolFuture<
		std::pin::Pin<Box<sc_transaction_pool_api::TransactionStatusStreamFor<Self>>>,
		Self::Error,
	> {
		let client_clone = self.1.clone();
		let pool = self.0.clone();
		let at_clone = *at;
		async move {
			match Self::check_extrinsics(client_clone, &vec![xt.clone()]).await {
				Ok(_) => pool.submit_and_watch(&at_clone, source, xt).await,
				Err(e) => Err(e),
			}
		}
		.boxed()
	}

	fn ready_at(
		&self,
		at: sp_api::NumberFor<Self::Block>,
	) -> std::pin::Pin<
		Box<
			dyn futures::Future<
					Output = Box<
						dyn sc_transaction_pool_api::ReadyTransactions<
								Item = sc_service::Arc<Self::InPoolTransaction>,
							> + Send,
					>,
				> + Send,
		>,
	> {
		self.0.ready_at(at)
	}

	fn ready(
		&self,
	) -> Box<
		dyn sc_transaction_pool_api::ReadyTransactions<
				Item = sc_service::Arc<Self::InPoolTransaction>,
			> + Send,
	> {
		self.0.ready()
	}

	fn remove_invalid(
		&self,
		hashes: &[sc_transaction_pool_api::TxHash<Self>],
	) -> Vec<sc_service::Arc<Self::InPoolTransaction>> {
		self.0.remove_invalid(hashes)
	}

	fn status(&self) -> sc_transaction_pool_api::PoolStatus {
		self.0.status()
	}

	fn import_notification_stream(
		&self,
	) -> sc_transaction_pool_api::ImportNotificationStream<sc_transaction_pool_api::TxHash<Self>> {
		self.0.import_notification_stream()
	}

	fn on_broadcasted(
		&self,
		propagations: HashMap<sc_transaction_pool_api::TxHash<Self>, Vec<String>>,
	) {
		self.0.on_broadcasted(propagations)
	}

	fn hash_of(
		&self,
		xt: &sc_transaction_pool_api::TransactionFor<Self>,
	) -> sc_transaction_pool_api::TxHash<Self> {
		self.0.hash_of(xt)
	}

	fn ready_transaction(
		&self,
		hash: &sc_transaction_pool_api::TxHash<Self>,
	) -> Option<sc_service::Arc<Self::InPoolTransaction>> {
		self.0.ready_transaction(hash)
	}
}
impl MaintainedTransactionPool for NetworkTxPool {
	fn maintain(
		&self,
		event: sc_transaction_pool_api::ChainEvent<Self::Block>,
	) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send>> {
		self.0.maintain(event)
	}
}
