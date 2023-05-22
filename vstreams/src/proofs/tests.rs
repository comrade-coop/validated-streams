use crate::{
	errors::Error,
	proofs::{EventProofs, InMemoryEventProofs, RocksDbEventProofs, WitnessedEvent},
};
use sp_core::{sr25519::Public, H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;

// Note: Could use rstest or a custom macro here, but it's simpler to just repeat everything for now

#[test]
fn test_add_event_proof_in_mem() {
	test_add_event_proof(InMemoryEventProofs::create());
}
#[test]
fn test_add_event_proof_rocksdb() {
	let _ = std::fs::remove_dir_all("/tmp/test1");
	test_add_event_proof(RocksDbEventProofs::create("/tmp/test1"));
}
#[test]
fn test_get_proof_count_in_mem() {
	test_get_proof_count(InMemoryEventProofs::create());
}
#[test]
fn test_get_proof_count_rocksdb() {
	let _ = std::fs::remove_dir_all("/tmp/test2");
	test_get_proof_count(RocksDbEventProofs::create("/tmp/test2"));
}
#[test]
fn test_remove_stale_events_in_mem() {
	test_remove_stale_events(InMemoryEventProofs::create());
}
#[test]
fn test_remove_stale_events_rocksdb() {
	let _ = std::fs::remove_dir_all("/tmp/test3");
	test_remove_stale_events(RocksDbEventProofs::create("/tmp/test3"));
}

fn get_validator_list() -> [CryptoTypePublicPair; 1] {
	return [CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(1)))]
}
fn get_new_validator_list() -> [CryptoTypePublicPair; 1] {
	return [CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(2)))]
}
fn create_witnessed_event(event_id: H256) -> WitnessedEvent {
	WitnessedEvent {
		event_id,
		pub_key: CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(1))),
		signature: vec![],
	}
}

fn test_add_event_proof<P: EventProofs>(proofs: P) {
	let event_id = H256::repeat_byte(1);
	let witnessed_event = create_witnessed_event(event_id);

	assert!(proofs.add_event_proof(&witnessed_event).is_ok());
	// add again the same event
	assert!(proofs.add_event_proof(&witnessed_event).is_ok());
}

fn test_get_proof_count<P: EventProofs>(proofs: P) {
	let event_id = H256::repeat_byte(1);
	let validator_list = get_validator_list();
	let new_validator_list = get_new_validator_list();

	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(0));

	let witnessed_event = create_witnessed_event(event_id);
	let _ = proofs.add_event_proof(&witnessed_event);
	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(1));
	assert_eq!(proofs.get_event_proof_count(&event_id, &new_validator_list), Ok(0));
}

fn test_remove_stale_events<P: EventProofs>(proofs: P) {
	let event_id = H256::repeat_byte(1);
	let witnessed_event = create_witnessed_event(event_id);
	let validator_list = get_validator_list();
	let new_validator_list = get_new_validator_list();

	let _ = proofs.add_event_proof(&witnessed_event);

	assert!(proofs.purge_event_stale_signatures(&event_id, &validator_list).is_ok());
	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(1));

	assert!(proofs.purge_event_stale_signatures(&event_id, &new_validator_list).is_ok());
	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(0));
}
