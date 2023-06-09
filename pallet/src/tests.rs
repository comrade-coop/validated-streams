use crate::{self as pallet_validated_streams, mock::*};
use frame_support::{assert_err, assert_ok};
use sp_core::H256;
use sp_runtime::{
	traits::ValidateUnsigned,
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};

#[test]
fn test_validate_unsigned() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let event_id = H256::repeat_byte(0);
		let call =
			pallet_validated_streams::Call::<Test>::validate_event { event_id, proofs: None };

		assert_err!(
			ValidatedStreams::validate_unsigned(TransactionSource::External, &call),
			TransactionValidityError::Invalid(InvalidTransaction::Call)
		);
		assert_ok!(ValidatedStreams::validate_unsigned(TransactionSource::Local, &call));
		assert_ok!(ValidatedStreams::validate_unsigned(TransactionSource::InBlock, &call));

		#[cfg(feature = "on-chain-proofs")]
		crate::mock::onchain_mod::initialize();
		#[cfg(feature = "on-chain-proofs")]
		let proofs_map = Some(crate::mock::onchain_mod::proofs(&event_id));
		#[cfg(not(feature = "on-chain-proofs"))]
		let proofs_map = None;

		assert_ok!(ValidatedStreams::validate_event(RuntimeOrigin::none(), event_id, proofs_map));
		assert_err!(
			ValidatedStreams::validate_unsigned(TransactionSource::Local, &call),
			TransactionValidityError::Invalid(InvalidTransaction::Stale)
		);
	})
}

/// dispatch an event to the streams StorageMap and check whether an en event has been raised
/// then dispatch the same event to verify Error handling since duplicates are not allowed
#[cfg(not(feature = "on-chain-proofs"))]
#[test]
fn it_adds_event() {
	new_test_ext().execute_with(|| {
		// Go past genesis block so events get deposited
		System::set_block_number(1);
		let event_id = H256::repeat_byte(0);
		assert!(!ValidatedStreams::verify_event(event_id));
		// Dispatch an extrinsic
		// signature should not matter since it should pass through validate_unsigned.
		assert_ok!(ValidatedStreams::validate_event(RuntimeOrigin::none(), event_id, None));
		assert_eq!(ValidatedStreams::get_all_events(), vec![event_id]);
		assert!(ValidatedStreams::verify_event(event_id));
		System::assert_last_event(
			pallet_validated_streams::Event::ValidatedEvent { event_id }.into(),
		);
		//double check the first block events
		assert_eq!(ValidatedStreams::get_block_events(1), vec![event_id]);
		//dispatch an extrinsic with an already validated event
		assert_err!(
			ValidatedStreams::validate_event(RuntimeOrigin::root(), event_id, None),
			pallet_validated_streams::Error::<Test>::AlreadyValidated
		);
	})
}

#[cfg(feature = "on-chain-proofs")]
#[test]
fn it_validates_event() {
	use crate::mock::onchain_mod::*;
	new_test_ext().execute_with(|| {
		// Go past genesis block so events get deposited
		System::set_block_number(1);
		let event_id = H256::repeat_byte(0);
		initialize();
		let proofs_map = proofs(&event_id);
		assert!(!ValidatedStreams::verify_event(event_id));
		// Dispatch an extrinsic
		// signature should not matter since it should pass through validate_unsigned.
		assert_ok!(ValidatedStreams::validate_event(
			RuntimeOrigin::none(),
			event_id,
			Some(proofs_map.clone())
		));
		assert_eq!(ValidatedStreams::get_all_events(), vec![event_id]);
		assert!(ValidatedStreams::verify_event(event_id));
		System::assert_last_event(
			pallet_validated_streams::Event::ValidatedEvent { event_id }.into(),
		);
		//dispatch an extrinsic with an already validated event
		assert_err!(
			ValidatedStreams::validate_event(
				RuntimeOrigin::root(),
				event_id,
				Some(proofs_map.clone())
			),
			pallet_validated_streams::Error::<Test>::AlreadyValidated
		);
		//corrupt a signature
		let event_id = H256::repeat_byte(1);
		let mut proofs_map = proofs(&event_id);
		*proofs_map
			.get_mut(&proofs_map.iter().next().unwrap().0.clone())
			.unwrap()
			.as_mut()
			.get_mut(0)
			.unwrap() += 1;
		assert_err!(
			ValidatedStreams::validate_event(
				RuntimeOrigin::root(),
				event_id,
				Some(proofs_map.clone())
			),
			pallet_validated_streams::Error::<Test>::InvalidProof
		);
		//inject an unrecognized authority proof
		let unrecognized_authority = KEYSTORE.sr25519_generate_new(AURA, None).unwrap();
		proofs_map
			.try_insert(
				unrecognized_authority,
				KEYSTORE
					.sign_with(
						AURA,
						&CryptoTypePublicPair::from(unrecognized_authority),
						event_id.as_bytes(),
					)
					.unwrap()
					.unwrap()
					.as_slice()
					.try_into()
					.unwrap(),
			)
			.unwrap();
		assert_err!(
			ValidatedStreams::validate_event(
				RuntimeOrigin::root(),
				event_id,
				Some(proofs_map.clone())
			),
			pallet_validated_streams::Error::<Test>::UnrecognizedAuthority
		);
		//provide unsifficient amount of proofs by removing two proofs since target is 3
		let mut proofs_map = proofs(&event_id);
		proofs_map.remove(&proofs_map.iter().next().unwrap().0.clone());
		proofs_map.remove(&proofs_map.iter().next().unwrap().0.clone());
		assert_err!(
			ValidatedStreams::validate_event(
				RuntimeOrigin::root(),
				event_id,
				Some(proofs_map.clone())
			),
			pallet_validated_streams::Error::<Test>::NotEnoughProofs
		);

		//provide no proofs
		assert_err!(
			ValidatedStreams::validate_event(RuntimeOrigin::root(), event_id, None),
			pallet_validated_streams::Error::<Test>::NoProofs
		);
	})
}
