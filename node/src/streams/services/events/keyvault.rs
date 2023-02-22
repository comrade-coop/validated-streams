use crate::service::FullClient;
use futures::StreamExt;
use node_runtime::opaque::BlockId;
use sc_client_api::BlockchainEvents;
use sc_service::Arc;
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::AuraApi;
use sp_core::{sr25519::Public, ByteArray};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, KeyTypeId};
use std::io::Error;
pub struct KeyVault {
	pub keystore: Arc<dyn CryptoStore>,
	pub keys: CryptoTypePublicPair,
	pub pubkey: Public,
}
impl KeyVault {
	/// this blocking method returns only when node has been added as a validator, by listening to
	/// incoming finalized blocks and checking whether the node has been added in the list of
	/// authorities or not.
	pub async fn new(
		keystore: Arc<dyn CryptoStore>,
		client: Arc<FullClient>,
		key_type: KeyTypeId,
	) -> Result<KeyVault, Error> {
		loop {
			client.finality_notification_stream().select_next_some().await;
			let sr25519_keys = keystore.sr25519_public_keys(key_type).await;
			if let Some(pubkey) = sr25519_keys
				.into_iter()
				.find(|key| KeyVault::validators_pubkeys(client.clone()).contains(key))
			{
				log::info!("node is currently a validator according to the latest finalized block");
				let keys = keystore
					.keys(key_type)
					.await
					.unwrap()
					.get(0)
					.expect("failed retreiving validator keypair from keystore")
					.clone();
				return Ok(KeyVault { keystore, keys, pubkey })
			}
		}
	}
	/// returns a list of sr25519 validators public keys
	pub fn validators_pubkeys(client: Arc<FullClient>) -> Vec<Public> {
		let block_id = BlockId::Number(client.chain_info().best_number);
		let authority_ids = client
			.runtime_api()
			.authorities(&block_id)
			.expect("failed retreiving authorities public keys");
		authority_ids
			.iter()
			.map(|pubkey| Public::from_slice(pubkey.as_slice()).unwrap())
			.collect()
	}
}
