//! # Validated Streams Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ### Dispatchable Functions
//! * [validate_event](pallet/struct.Pallet.html#method.validate_event)
#![cfg_attr(not(feature = "std"), no_std)]
// Re-export pallet items so that they can be accessed from the crate namespace.
#[cfg(test)]
mod tests;
pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {

	use frame_support::{
		pallet_prelude::{ValidTransaction, *},
		BoundedBTreeMap, BoundedVec,
	};
	use frame_system::pallet_prelude::*;
	use sp_api;
	#[cfg(feature = "on-chain-proofs")]
	use sp_core::sr25519::Signature;
	use sp_core::{sr25519::Public, H256};
	#[cfg(feature = "on-chain-proofs")]
	use sp_runtime::app_crypto::RuntimePublic;
	pub use sp_runtime::traits::Extrinsic;
	use sp_runtime::RuntimeAppPublic;
	use sp_std::vec::Vec;
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		#[pallet::constant]
		type SignatureLength: Get<u32>;
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
		UnrecognizedAuthority,
	}

	#[cfg(not(feature = "on-chain-proofs"))]
	#[pallet::storage]
	pub(super) type Streams<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, T::BlockNumber>;

	#[cfg(feature = "on-chain-proofs")]
	#[pallet::storage]
	pub(super) type OnStreams<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		BoundedBTreeMap<Public, BoundedVec<u8, T::SignatureLength>, T::VSMaxAuthorities>,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Used to validate an event.
		/// Checks if the event has already been validated.
		/// If so, it raise an `AlreadyValidated` event.
		/// If not, it inserts the event and the current block into the storage and raise a
		/// `ValidatedEvent` event.

		#[pallet::weight(0)]
		pub fn validate_event(
			origin: OriginFor<T>,
			event_id: T::Hash,
			proofs: Option<
				BoundedBTreeMap<Public, BoundedVec<u8, T::SignatureLength>, T::VSMaxAuthorities>,
			>,
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
			_proofs: Option<
				BoundedBTreeMap<Public, BoundedVec<u8, T::SignatureLength>, T::VSMaxAuthorities>,
			>,
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
			event_proofs: Option<
				BoundedBTreeMap<Public, BoundedVec<u8, T::SignatureLength>, T::VSMaxAuthorities>,
			>,
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
				for (key, sig) in &proofs {
					if let Some(signature) = Signature::from_slice(sig.as_slice()) {
						ensure!(key.verify(&event_id, &signature), Error::<T>::InvalidProof);
					} else {
						return Err(Error::<T>::BadSignature.into())
					}
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
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			ValidTransaction::with_tag_prefix("validated_streams")
				.and_provides(call.encode())
				.build()
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
	sp_api::decl_runtime_apis! {
		/// Get extrinsic ids from a vector of extrinsics
		/// that should be used to quickly retrieve all the event ids (hashes) given a vector of extrinsics
		/// currently used to inspect the proposed block event ids and whether they are witnessed offchain or not
		pub trait ExtrinsicDetails<T,R> where T:Extrinsic + Decode, R:Config{
			#[allow(clippy::ptr_arg)]
			fn get_extrinsic_ids(extrinsics: &Vec<Block::Extrinsic>) -> Vec<H256>;
			fn create_unsigned_extrinsic(event_id:H256,event_proofs:Option<BoundedBTreeMap<Public,BoundedVec<u8,R::SignatureLength>,R::VSMaxAuthorities>>)-> T;
		}
	}
}
