use super::{EventServiceBlockState, WitnessedEvent};
use sc_keystore::LocalKeystore;
use sp_core::{sr25519::Public, H256};
use sp_keystore::CryptoStore;
use sp_runtime::{app_crypto::CryptoTypePublicPair, key_types::AURA};

#[tokio::test]
async fn test_verify_events() {
	//simple witnessed eventevent
	let keystore = LocalKeystore::in_memory();
	let event_id = H256::repeat_byte(0);
	let key = keystore.sr25519_generate_new(AURA, None).await.unwrap();
	let witnessed_event = create_witnessed_event(event_id, &keystore, key.clone()).await;
	let validators_list = vec![CryptoTypePublicPair::from(key)];
	let block_state = EventServiceBlockState::new(validators_list);

	let result = block_state.verify_witnessed_event_origin(witnessed_event.clone());
	assert_eq!(result.unwrap(), witnessed_event);

	let mut empty_sig_event = witnessed_event.clone();
	empty_sig_event.signature = vec![];
	let result = block_state.verify_witnessed_event_origin(empty_sig_event);
	assert!(result.is_err());

	//create an invalid signature
	let mut invalid_sig_event = witnessed_event.clone();
	invalid_sig_event.signature.push(8);
	let result = block_state.verify_witnessed_event_origin(invalid_sig_event);
	assert!(result.is_err());

	let mut bad_sig_event = witnessed_event.clone();
	*bad_sig_event.signature.get_mut(8).unwrap() += 1;
	let result = block_state.verify_witnessed_event_origin(bad_sig_event);
	assert!(result.is_err());

	let mut invalid_key_event = witnessed_event.clone();
	invalid_key_event.pub_key = vec![];
	let result = block_state.verify_witnessed_event_origin(invalid_key_event);
	assert!(result.is_err());

	//receive an event from a non-validator
	let no_validators_block_state = EventServiceBlockState::new(vec![]);
	let result = no_validators_block_state.verify_witnessed_event_origin(witnessed_event.clone());
	assert!(result.is_err());
}

async fn create_witnessed_event(
	event_id: H256,
	keystore: &LocalKeystore,
	key: Public,
) -> WitnessedEvent {
	let signature = keystore
		.sign_with(AURA, keystore.keys(AURA).await.unwrap().get(0).unwrap(), event_id.as_bytes())
		.await
		.unwrap()
		.unwrap();
	WitnessedEvent { event_id, pub_key: key.clone().to_vec(), signature }
}
