use node_runtime::opaque::BlockId;
use sc_service::Arc;
use sp_core::{sr25519::Public, H256, ByteArray};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, key_types::AURA, KeyTypeId};
use crate::service::FullClient;
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::AuraApi;

pub struct KeyVault {
	pub keystore: Arc<dyn CryptoStore>,
	pub keys: CryptoTypePublicPair,
	pub pubkey: Public,
}
impl KeyVault{
    /// Create a new KeyVault by getting the list of authorities from Aura and retreiving the apporiate
    /// keys that are used by the current validator
    pub async fn new(keystore: Arc<dyn CryptoStore>, client:Arc<FullClient> ,key_type:KeyTypeId) -> KeyVault{
        let sr25519_keys = keystore.sr25519_public_keys(sp_core::crypto::key_types::AURA).await;
        let pubkey = sr25519_keys
        .into_iter()
        .find(|key| KeyVault::validators_pubkeys(client.clone()).contains(key))
        .expect("Self pubkey was not found in the list of validators");
        // when should one have more than one key for one consensus algorithm?
        let keys = keystore
            .keys(key_type)
            .await
            .unwrap()
            .get(0)
            .expect("failed retreiving validator keypair from keystore")
            .clone();
        println!("all aura keys{:?}",keys);
        KeyVault { keystore, keys, pubkey }
    }
    pub fn validators_pubkeys(client: Arc<FullClient>)-> Vec<Public>
    {
        let block_id = BlockId::Number(client.chain_info().best_number);
        let authority_ids = client
            .runtime_api()
            .authorities(&block_id)
            .ok()
            .expect("failed retreiving authorities public keys");
        authority_ids
            .iter()
            .map(|pubkey| Public::from_h256(H256::from_slice(pubkey.as_slice())))
            .collect()
    }
}
