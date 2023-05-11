#![allow(unused_imports)]
use super::*;
use crate::{Config, Pallet as pallet_validated_streams};
use frame_benchmarking::{benchmarks, BenchmarkError, Vec};
use frame_support::{ensure, traits::ConstU32, BoundedBTreeMap, BoundedVec};
use frame_system::{pallet_prelude::*, RawOrigin};
use sp_core::{
	crypto::key_types::AURA,
	sr25519::{Public, Signature},
	H256,
};
use sp_io::crypto::{sr25519_generate, sr25519_sign};
use sp_runtime::{app_crypto::RuntimePublic, RuntimeAppPublic};
#[cfg(not(feature = "on-chain-proofs"))]
benchmarks! {
	validate_event {
		let event_id: T::Hash = T::Hash::default();
	}: _(RawOrigin::None,event_id,None)
	verify {
		assert_eq!(pallet_validated_streams::<T>::verify_event(event_id), true);
	}
	impl_benchmark_test_suite!(pallet_validated_streams, crate::mock::new_test_ext(), crate::mock::Test)
}
#[cfg(feature = "on-chain-proofs")]
benchmarks! {
	on_chain_proofs {
		let event_id = H256::default();
		let event_hash = T::Hash::default();
		// type ProofsMap= BoundedBTreeMap<sp_core::sr25519::Public, BoundedVec<u8, ConstU32<64>>, ConstU32<32>>;
		let event_proofs = {
			let mut proofs = BoundedBTreeMap::new();
			for i in 0..32{
				let key = sr25519_generate(AURA,None);
				let signature :BoundedVec<_,_>= sr25519_sign(AURA,&key,&event_id.as_bytes()).unwrap().0.to_vec().try_into().unwrap();
				proofs.try_insert(key,signature).unwrap();
			}
			Some(proofs)
		};
	}: {
		// not going to use this one since it represent real authorities in the actual runtime
		// just benchmark db read
		if let Some(proofs) = event_proofs{
			let authorities: Vec<Public> = T::authorities()
				.into_iter()
				.map(|id| Public::from_h256(H256::from_slice(id.to_raw_vec().as_slice())))
				.collect();
			let target = (2 * ((authorities.len() - 1) / 3) + 1) as u16;
			ensure!(!OnStreams::<T>::contains_key(event_hash), BenchmarkError::Stop("Already validated event"));
			// worst case is finding all the keys in authorities!
			for key in proofs.keys(){
				if !proofs.contains_key(key) {
					return Err(BenchmarkError::Stop("Unrecognized Authority"))
				}
			}
			let mut proof_count =0;
			for (key,sig) in &proofs{
				if let Some(signature) = Signature::from_slice(sig.as_slice()) {
					ensure!(key.verify(&event_id, &signature), BenchmarkError::Stop("Invalid proof"));
					proof_count+=1;
				} else {
					return Err(BenchmarkError::Stop("Bad Signature"))
				}
			}
			if proof_count < target{
				return Err(BenchmarkError::Stop("not enough proofs"));
			}
			OnStreams::<T>::insert(event_hash, proofs);
			pallet_validated_streams::<T>::deposit_event(Event::ValidatedEvent { event_id:event_hash });
		}
	}
}
