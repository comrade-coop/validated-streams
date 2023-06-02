#![cfg_attr(not(feature = "std"), no_std)]
//! # Validated Streams Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ### Dispatchable Functions
//! * [validate_event](pallet/struct.Pallet.html#method.validate_event)
// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;
#[cfg(test)]
pub mod tests;

#[cfg(test)]
pub mod mock;

pub mod weights;
pub use weights::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::{ValidTransaction, *},
		BoundedBTreeMap, BoundedVec,
	};
	use frame_system::pallet_prelude::*;
	use sp_api;
	use sp_core::{
		sr25519::{Public, Signature},
		H256,
	};
	#[cfg(feature = "on-chain-proofs")]
	use sp_runtime::app_crypto::RuntimePublic;
	pub use sp_runtime::traits::Extrinsic;
	use sp_runtime::RuntimeAppPublic;
	use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type WeightInfo: WeightInfo;
		type VSAuthorityId: Member
			+ Parameter
			+ RuntimeAppPublic
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;
		#[pallet::constant]
		type VSMaxAuthorities: Get<u32>;
		fn authorities() -> BoundedVec<Self::VSAuthorityId, Self::VSMaxAuthorities>;
	}
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An Event has been validated
		ValidatedEvent { event_id: T::Hash },
	}
	#[pallet::error]
	pub enum Error<T> {
		/// The event was already found in the Streams StorageMap which means its already validated
		AlreadyValidated,
		BadSignature,
		InvalidProof,
		NoProofs,
		NotEnoughProofs,
		UnrecognizedAuthority,
	}

	type ProofsMap<T> = BoundedBTreeMap<Public, Signature, <T as Config>::VSMaxAuthorities>;

	#[cfg(not(feature = "on-chain-proofs"))]
	#[pallet::storage]
	pub(super) type Streams<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, T::BlockNumber>;

	#[cfg(feature = "on-chain-proofs")]
	#[pallet::storage]
	pub(super) type OnStreams<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, ProofsMap<T>>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Used to validate an event.
		/// Checks if the event has already been validated.
		/// If so, it raise an `AlreadyValidated` event.
		/// If not, it inserts the event and the current block into the storage and raise a
		/// `ValidatedEvent` event.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::validate_event())]
		pub fn validate_event(
			origin: OriginFor<T>,
			event_id: T::Hash,
			proofs: Option<ProofsMap<T>>,
		) -> DispatchResult {
			// indirection because pallet::call does not support cfg feature macro yet
			Pallet::<T>::validate_event_impl(origin, event_id, proofs)
		}
	}
	impl<T: Config> Pallet<T> {
		#[cfg(not(feature = "on-chain-proofs"))]
		pub fn validate_event_impl(
			_origin: OriginFor<T>,
			event_id: T::Hash,
			_proofs: Option<ProofsMap<T>>,
		) -> DispatchResult {
			let current_block = <frame_system::Pallet<T>>::block_number();
			ensure!(!Streams::<T>::contains_key(event_id), Error::<T>::AlreadyValidated);
			Streams::<T>::insert(event_id, current_block);
			Self::deposit_event(Event::ValidatedEvent { event_id });
			Ok(())
		}
		#[cfg(feature = "on-chain-proofs")]
		pub fn validate_event_impl(
			_origin: OriginFor<T>,
			event_id: T::Hash,
			event_proofs: Option<ProofsMap<T>>,
		) -> DispatchResult {
			let authorities: Vec<Public> = T::authorities()
				.into_iter()
				.map(|id| Public::from_h256(H256::from_slice(id.to_raw_vec().as_slice())))
				.collect();
			if let Some(proofs) = event_proofs {
				ensure!(!OnStreams::<T>::contains_key(event_id), Error::<T>::AlreadyValidated);
				for key in proofs.keys() {
					if !authorities.contains(key) {
						return Err(Error::<T>::UnrecognizedAuthority.into())
					}
				}

				let target = (2 * ((authorities.len() - 1) / 3) + 1) as u16;
				let mut proof_count = 0;
				for (key, signature) in &proofs {
					ensure!(key.verify(&event_id, signature), Error::<T>::InvalidProof);
					proof_count += 1;
				}
				if proof_count < target {
					return Err(Error::<T>::NotEnoughProofs.into())
				}
				OnStreams::<T>::insert(event_id, proofs);
				Self::deposit_event(Event::ValidatedEvent { event_id });
				Ok(())
			} else {
				Err(Error::<T>::NoProofs.into())
			}
		}
	}
	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if source != TransactionSource::Local && source != TransactionSource::InBlock {
				return Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			}
			match call{
				Self::Call::validate_event {
					event_id: call_data,
					proofs: _,
				} => {
					if Self::verify_event(*call_data) == true{ return Err(TransactionValidityError::Invalid(InvalidTransaction::Stale))}else{
						ValidTransaction::with_tag_prefix("validated_streams")
							.and_provides(call.encode())
							.and_provides(*call_data)
							.propagate(false)
							.build()
					}
					},
					_ =>{
						return Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
					}

			}
		}
	}
	#[cfg(not(feature = "on-chain-proofs"))]
	impl<T: Config> Pallet<T> {
		/// This function is used to get all events from the Streams StorageMap.
		pub fn get_all_events() -> Vec<T::Hash> {
			Streams::<T>::iter().map(|(k, _)| k).collect()
		}
		/// This function is used to get all events of a specific block.
		pub fn get_block_events(block_number: T::BlockNumber) -> Vec<T::Hash> {
			Streams::<T>::iter()
				.filter(|(_, bn)| *bn == block_number)
				.map(|(k, _)| k)
				.collect()
		}
		/// verify whether an event is valid or not
		pub fn verify_event(event_id: T::Hash) -> bool {
			Streams::<T>::contains_key(event_id)
		}
	}
	#[cfg(feature = "on-chain-proofs")]
	impl<T: Config> Pallet<T> {
		/// This function is used to get all events from the Streams StorageMap.
		pub fn get_all_events() -> Vec<T::Hash> {
			OnStreams::<T>::iter().map(|(k, _)| k).collect()
		}
		/// verify whether an event is valid or not
		pub fn verify_event(event_id: T::Hash) -> bool {
			OnStreams::<T>::contains_key(event_id)
		}
	}
	sp_api::decl_runtime_apis! {
		/// Get extrinsic ids from a vector of extrinsics
		/// that should be used to quickly retrieve all the event ids (hashes) given a vector of
		/// extrinsics currently used to inspect the proposed block event ids and whether they are
		/// witnessed offchain or not
		pub trait ExtrinsicDetails
		{
			#[allow(clippy::ptr_arg)]
			fn get_extrinsic_ids(extrinsics: &Vec<Block::Extrinsic>) -> Vec<H256>;
			fn create_unsigned_extrinsic(
				event_id: H256,
				event_proofs: Option<BTreeMap<Public, Signature>>,
			) -> Block::Extrinsic;
			fn is_witnessed_event_extrinsic(extrinsic: Block::Extrinsic) -> bool; // Use ValidateUnsigned instead?
		}
	}
}
