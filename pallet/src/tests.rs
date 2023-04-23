use crate::{self as pallet_validated_streams, Config};
use crate::mock::*;
use frame_support::{assert_err, assert_ok, BoundedBTreeMap, BoundedVec};
use sp_core::{H256, sr25519::Public};
use sp_runtime::key_types::AURA;
use sp_keystore::SyncCryptoStore;
use sp_core::crypto::CryptoTypePublicPair;
/// dispatch an event to the streams StorageMap and check whether an en event has been raised
/// then dispatch the same event to verify Error handling since duplicates are not allowed
#[cfg(not(feature = "on-chain-proofs"))]
#[test]
fn it_adds_event() {
    new_test_ext().execute_with(|| {
        // Go past genesis block so events get deposited
        System::set_block_number(1);
        let event_id = H256::repeat_byte(0);
        assert_eq!(ValidatedStreams::verify_event(event_id), false);
        // Dispatch an extrinsic
        // signature should not matter since it should pass through validate_unsigned.
        assert_ok!(ValidatedStreams::validate_event(Origin::none(), event_id, None));
        assert_eq!(ValidatedStreams::get_all_events(), vec![event_id]);
        assert_eq!(ValidatedStreams::verify_event(event_id), true);
        System::assert_last_event(
            pallet_validated_streams::Event::ValidatedEvent { event_id }.into(),
            );
        //double check the first block events
        assert_eq!(ValidatedStreams::get_block_events(1), vec![event_id]);
        //dispatch an extrinsic with an already validated event
        assert_err!(
            ValidatedStreams::validate_event(Origin::root(), event_id, None),
            pallet_validated_streams::Error::<Test>::AlreadyValidated
            );
    })
}

#[cfg(feature = "on-chain-proofs")]
#[test]
fn it_validates_event() {
    new_test_ext().execute_with(|| {
        // Go past genesis block so events get deposited
        System::set_block_number(1);
        let event_id = H256::repeat_byte(0);
        let mut keys = Vec::new();
        initialize(&mut keys);
        let proofs_map = proofs(&event_id,&keys);
        assert_eq!(ValidatedStreams::verify_event(event_id), false);
        // Dispatch an extrinsic
        // signature should not matter since it should pass through validate_unsigned.
        assert_ok!(ValidatedStreams::validate_event(Origin::none(), event_id,Some(proofs_map.clone())));
        assert_eq!(ValidatedStreams::get_all_events(), vec![event_id]);
        assert_eq!(ValidatedStreams::verify_event(event_id), true);
        System::assert_last_event(
            pallet_validated_streams::Event::ValidatedEvent { event_id }.into(),
            );
        //dispatch an extrinsic with an already validated event
        assert_err!(
            ValidatedStreams::validate_event(Origin::root(), event_id,Some(proofs_map.clone())),
            pallet_validated_streams::Error::<Test>::AlreadyValidated
            );
        //corrupt a signature
        let event_id = H256::repeat_byte(1);
        let mut proofs_map = proofs(&event_id,&keys);
        *proofs_map.get_mut(keys.get(0).unwrap()).unwrap().get_mut(0).unwrap() +=1;
        assert_err!(
            ValidatedStreams::validate_event(Origin::root(), event_id,Some(proofs_map.clone())),
            pallet_validated_streams::Error::<Test>::InvalidProof
            );
        //inject an unrecognized authority proof
        let unrecognized_authority =  KEYSTORE.sr25519_generate_new(AURA, None).unwrap();
        proofs_map.try_insert(unrecognized_authority,BoundedVec::default()).unwrap();
        assert_err!(
            ValidatedStreams::validate_event(Origin::root(), event_id,Some(proofs_map.clone())),
            pallet_validated_streams::Error::<Test>::UnrecognizedAuthority
            );
        //privide unsifficient amount of proofs by removing two proofs since target is 3
        let mut proofs_map = proofs(&event_id,&keys);
        proofs_map.remove(keys.get(0).unwrap());
        proofs_map.remove(keys.get(1).unwrap());
        assert_err!(
            ValidatedStreams::validate_event(Origin::root(), event_id,Some(proofs_map.clone())),
            pallet_validated_streams::Error::<Test>::NotEnoughProofs
            );

        
    })
}
#[cfg(feature = "on-chain-proofs")]
pub fn proofs(event_id: &H256,keys: &Vec<Public>) -> BoundedBTreeMap<Public, BoundedVec<u8, <Test as Config>::SignatureLength>, <Test as Config>::VSMaxAuthorities>{
    let mut proofs = BoundedBTreeMap::new();
    for i in 0..4
    {
        let signature = KEYSTORE
            .sign_with(AURA, &PAIRS.lock().unwrap().get(i).unwrap(),event_id.as_bytes())
            .unwrap()
            .unwrap();
        proofs.try_insert(keys.get(i).unwrap().clone(),BoundedVec::truncate_from(signature.to_vec())).unwrap();
    }
    proofs
}
#[cfg(feature = "on-chain-proofs")]
fn initialize(keys: &mut Vec<Public>)
{
    for _ in 0..4
    {
        let key = KEYSTORE.sr25519_generate_new(AURA, None).unwrap();
        keys.push(key);
        let pair = CryptoTypePublicPair::from(key);
        PAIRS.lock().unwrap().push(pair.clone());
    }

}
