use crate::service::FullClient;
use node_runtime::opaque::BlockId;
use sc_service::Arc;
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::AuraApi;
use sp_core::{sr25519::Public, ByteArray, H256};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, KeyTypeId};
use std::io::Error;
pub struct KeyVault {
	pub keystore: Arc<dyn CryptoStore>,
	pub keys: CryptoTypePublicPair,
	pub pubkey: Public,
}
impl KeyVault {
	/// Create a new KeyVault by getting the list of authorities from Aura and retreiving the
	/// apporiate keys that are used by the current validator
	pub async fn new(
		keystore: Arc<dyn CryptoStore>,
		client: Arc<FullClient>,
		key_type: KeyTypeId,
	) -> Result<KeyVault, Error> {
		let sr25519_keys = keystore.sr25519_public_keys(key_type).await;
		if let Some(pubkey) = sr25519_keys
			.into_iter()
			.find(|key| KeyVault::validators_pubkeys(client.clone()).contains(key))
		{
			// when should one have more than one key for one consensus algorithm?
			let keys = keystore
				.keys(key_type)
				.await
				.unwrap()
				.get(0)
				.expect("failed retreiving validator keypair from keystore")
				.clone();
			Ok(KeyVault { keystore, keys, pubkey })
		} else {
			Err(Error::new(
				std::io::ErrorKind::NotFound,
				"Self pubkey was not found in the list of validators".to_string(),
			))
		}
	}
	pub fn validators_pubkeys(client: Arc<FullClient>) -> Vec<Public> {
		let block_id = BlockId::Number(client.chain_info().best_number);
		let authority_ids = client
			.runtime_api()
			.authorities(&block_id)
			.expect("failed retreiving authorities public keys");
		authority_ids
			.iter()
			.map(|pubkey| Public::from_h256(H256::from_slice(pubkey.as_slice())))
			.collect()
	}
}
