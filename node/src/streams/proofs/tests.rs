use sp_core::H256;

use crate::streams::proofs::{EventProofs, InMemoryEventProofs, WitnessedEvent};
#[test]
fn test_add_event_proof() {
	let event_id = H256::repeat_byte(0);
	let witnessed_event = create_witnessed_event(event_id);
	let origin = b"alice".to_vec();

	let proofs = InMemoryEventProofs::create();
	let result = proofs.add_event_proof(&witnessed_event, origin.clone());
	assert!(result.is_ok());

	let result = proofs.add_event_proof(&witnessed_event, origin);
	assert!(result.is_err());
}

#[test]
fn test_get_proof_count() {
	let event_id = H256::repeat_byte(0);
	let proofs = InMemoryEventProofs::create();

	let result = proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(0));

	let witnessed_event = create_witnessed_event(event_id);
	let origin = b"alice".to_vec();
	let _ = proofs.add_event_proof(&witnessed_event, origin);

	let result = proofs.get_proof_count(event_id);
	assert_eq!(result, Ok(1));
}
fn create_witnessed_event(event_id: H256) -> WitnessedEvent {
	WitnessedEvent { event_id, pub_key: vec![], signature: vec![] }
}
