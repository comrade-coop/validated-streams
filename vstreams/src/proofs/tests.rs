use crate::proofs::{EventProofs, InMemoryEventProofs, ProofStore, WitnessedEvent};
use sp_core::{sr25519::Public, H256};
use sp_runtime::app_crypto::CryptoTypePublicPair;

#[test]
fn test_add_event_proof() {
	let event_id = H256::repeat_byte(0);
	let witnessed_event = create_witnessed_event(event_id);

	let proofs = ProofStore::create("/tmp/test");
	let in_mem_proofs = InMemoryEventProofs::create();

	let result = in_mem_proofs.add_event_proof(&witnessed_event);
	assert!(result.is_ok());
	let result = proofs.add_event_proof(&witnessed_event);
	assert!(result.is_ok());

	// add again the same event
	let result = proofs.add_event_proof(&witnessed_event);
	assert!(result.is_err());
	let result = in_mem_proofs.add_event_proof(&witnessed_event);
	assert!(result.is_err());
}
#[test]
fn test_get_proof_count() {
	let event_id = H256::repeat_byte(1);
	let proofs = ProofStore::create("/tmp/test2");
	let in_mem_proofs = InMemoryEventProofs::create();

	let result = proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(0));
	let result = in_mem_proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(0));

	let witnessed_event = create_witnessed_event(event_id);
	let _ = proofs.add_event_proof(&witnessed_event);
	let _ = in_mem_proofs.add_event_proof(&witnessed_event);

	let result = proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(1));
	let result = in_mem_proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(1));
}
#[test]
fn test_remove_stale_events() {
	let event_id = H256::repeat_byte(0);
	let witnessed_event = create_witnessed_event(event_id);
	let origin = CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(1)));
	let in_mem_proofs = InMemoryEventProofs::create();
	let proofs = ProofStore::create("/tmp/test3");
	let _ = proofs.add_event_proof(&witnessed_event);
	let _ = in_mem_proofs.add_event_proof(&witnessed_event);

	assert!(proofs.purge_stale_signatures(&vec![origin.clone()], &vec![event_id]).is_ok());
	assert!(in_mem_proofs.purge_stale_signatures(&vec![origin], &vec![event_id]).is_ok());
	assert_eq!(proofs.get_proof_count(event_id), Ok(1));
	assert_eq!(in_mem_proofs.get_proof_count(event_id), Ok(1));

	let mock_new_validator_list =
		vec![CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(2)))];
	assert!(proofs.purge_stale_signatures(&mock_new_validator_list, &vec![event_id]).is_ok());
	assert!(in_mem_proofs
		.purge_stale_signatures(&mock_new_validator_list, &vec![event_id])
		.is_ok());
	assert_eq!(proofs.get_proof_count(event_id), Ok(0));
	assert_eq!(in_mem_proofs.get_proof_count(event_id), Ok(0));
}

fn create_witnessed_event(event_id: H256) -> WitnessedEvent {
	WitnessedEvent {
		event_id,
		pub_key: CryptoTypePublicPair::from(Public::from_h256(H256::repeat_byte(1))),
		signature: vec![],
	}
}
