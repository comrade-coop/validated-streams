//! # Validated Streams Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ### Dispatchable Functions
//! * [validate_event](pallet/struct.Pallet.html#method.validate_event)
#![cfg_attr(not(feature = "std"), no_std)]
// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_api;
	use sp_core::H256;
	use sp_std::vec::Vec;
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
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
	}
	#[pallet::storage]
	pub(super) type Streams<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, (T::AccountId, T::BlockNumber)>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Used to validate an event.
		/// It Check that the extrinsic was signed and get the signer.
		/// It then checks if the event has already been validated.
		/// If so, it raise an `AlreadyValidated` event.
		/// If not, it inserts the event and its details into the storage and raise a
		/// `ValidatedEvent` event.
		#[pallet::weight(0)]
		pub fn validate_event(origin: OriginFor<T>, event_id: T::Hash) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let current_block = <frame_system::Pallet<T>>::block_number();
			ensure!(!Streams::<T>::contains_key(&event_id), Error::<T>::AlreadyValidated);
			Streams::<T>::insert(&event_id, (&sender, current_block));
			Self::deposit_event(Event::ValidatedEvent { event_id });
			Ok(())
		}
	}
	impl<T: Config> Pallet<T> {
		/// This function is used to get all events from the Streams StorageMap.
		pub fn get_all_events() -> Vec<T::Hash> {
			Streams::<T>::iter().map(|(k, _)| k).collect()
		}
		/// This function is used to get all events of a specific account.
		pub fn get_account_events(account_id: T::AccountId) -> Vec<T::Hash> {
			Streams::<T>::iter()
				.filter(|(_, (id, _))| *id == account_id)
				.map(|(k, _)| k)
				.collect()
		}
		/// This function is used to get all events of a specific block.
		pub fn get_block_events(block_number: T::BlockNumber) -> Vec<T::Hash> {
			Streams::<T>::iter()
				.filter(|(_, (_, bn))| *bn == block_number)
				.map(|(k, _)| k)
				.collect()
		}
		/// This function is used to get all events of a specific account at a specific block.
		pub fn get_account_events_at_block(
			account_id: T::AccountId,
			block_number: T::BlockNumber,
		) -> Vec<T::Hash> {
			Streams::<T>::iter()
				.filter(|(_, (id, bn))| *bn == block_number && *id == account_id)
				.map(|(k, _)| k)
				.collect()
		}
	}
	sp_api::decl_runtime_apis! {
	/// Get extrinsic ids from a vector of extrinsics
	/// that should be used to quickly retreive all the event ids (hashes) given a vector of extrinsics
	/// currently used to inspect the proposed block event ids and whether they are witnessed offchain or not
		pub trait ExtrinsicDetails{
			fn get_extrinsic_ids(extrinsics: &Vec<Block::Extrinsic>) -> Vec<H256>;
		}
	}
}