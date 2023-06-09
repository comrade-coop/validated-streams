use crate::proofs::{EventProofs, InMemoryEventProofs, RocksDbEventProofs, OffchainStorageEventProofs, WitnessedEvent};
use sp_core::{sr25519::Public, H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;
use sp_runtime::offchain::testing::TestPersistentOffchainDB;
use rstest::rstest;
use std::sync::atomic::{AtomicUsize, Ordering};

fn in_memory_proofs() -> impl EventProofs {
	InMemoryEventProofs::create()
}


static ROCKSDB_INSTANCE: AtomicUsize = AtomicUsize::new(1);
fn rocksdb_proofs() -> impl EventProofs {
	let path = format!("/tmp/testvstreamsrocksdb{}", ROCKSDB_INSTANCE.fetch_add(1, Ordering::SeqCst));
	let _ = RocksDbEventProofs::destroy(&path);
	RocksDbEventProofs::create(&path)
}

fn offchain_proofs() -> impl EventProofs {
	OffchainStorageEventProofs::create(TestPersistentOffchainDB::new())
}


#[rstest]
#[case(in_memory_proofs())]
#[case(rocksdb_proofs())]
#[case(offchain_proofs())]
fn test_add_event_proof(#[case] proofs: impl EventProofs) {
	let event_id = H256::repeat_byte(1);
	let witnessed_event = create_witnessed_event(event_id);

	assert!(proofs.add_event_proof(&witnessed_event).is_ok());
	// add again the same event
	assert!(proofs.add_event_proof(&witnessed_event).is_ok());
}

#[rstest]
#[case(in_memory_proofs())]
#[case(rocksdb_proofs())]
#[case(offchain_proofs())]
fn test_get_proof_count(#[case] proofs: impl EventProofs) {
	let event_id = H256::repeat_byte(1);
	let validator_list = get_validator_list();
	let new_validator_list = get_new_validator_list();

	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(0));

	let witnessed_event = create_witnessed_event(event_id);
	let _ = proofs.add_event_proof(&witnessed_event);
	assert_eq!(proofs.get_event_proof_count(&event_id, &validator_list), Ok(1));
	assert_eq!(proofs.get_event_proof_count(&event_id, &new_validator_list), Ok(0));
}

#[rstest]
#[case(in_memory_proofs())]
#[case(rocksdb_proofs())]
#[case(offchain_proofs())]
fn test_remove_stale_events(#[case] proofs: impl EventProofs) {
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
