use sc_keystore::LocalKeystore;
use sp_core::{sr25519::Public, H256};
use sp_keystore::CryptoStore;
use sp_runtime::key_types::AURA;
use super::{EventService, WitnessedEvent};

#[tokio::test]
async fn test_verify_events() {
	//simple witnessed eventevent
	let keystore = LocalKeystore::in_memory();
	let event_id = H256::repeat_byte(0);
	let key =  keystore.sr25519_generate_new(AURA, None).await.unwrap();
	let witnessed_event = create_witnessed_event(event_id, &keystore,key.clone()).await;
	let validators_list = vec![key];
	
    let result = EventService::verify_witnessed_event(&validators_list, &witnessed_event);
	assert_eq!(result.unwrap(),true);
    
    let mut empty_sig_event = witnessed_event.clone();
    empty_sig_event.signature =vec![]; 
    let result = EventService::verify_witnessed_event(&validators_list, &empty_sig_event);
    assert!(result.is_err());
    
    //create an invalid signature
    let mut invalid_sig_event = witnessed_event.clone();
    invalid_sig_event.signature.push(8);
    let result = EventService::verify_witnessed_event(&validators_list, &invalid_sig_event);
    assert!(result.is_err());
    
    let mut bad_sig_event = witnessed_event.clone();
    *bad_sig_event.signature.get_mut(8).unwrap()+=1; 
    let result = EventService::verify_witnessed_event(&validators_list, &bad_sig_event);
    assert_eq!(result.unwrap(),false);
    
    //receive an event from a non-validator
    let second_list = vec![];
    let result = EventService::verify_witnessed_event(&second_list, &witnessed_event);
    assert_eq!(result.unwrap(),false);
    

    let mut invalid_key_event = witnessed_event.clone();
    invalid_key_event.pub_key = vec![];
    let result = EventService::verify_witnessed_event(&validators_list, &invalid_key_event);
    assert!(result.is_err());


}

async fn create_witnessed_event(event_id: H256, keystore: &LocalKeystore,key:Public) -> WitnessedEvent {
    let signature= keystore.sign_with(AURA, keystore.keys(AURA).await.unwrap().get(0).unwrap(), event_id.as_bytes()).await.unwrap().unwrap();
    WitnessedEvent { event_id, pub_key:key.clone().to_vec(), signature }
}
