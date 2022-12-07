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
		ValidatedEvent { event_id: T::Hash },
	}
	#[pallet::error]
	pub enum Error<T> {
		AlreadyValidated,
	}
	#[pallet::storage]
	pub(super) type Streams<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, (T::AccountId, T::BlockNumber)>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		pub fn validate_event(origin: OriginFor<T>, event_id: T::Hash) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			let sender = ensure_signed(origin)?;
			let current_block = <frame_system::Pallet<T>>::block_number();
			ensure!(!Streams::<T>::contains_key(&event_id), Error::<T>::AlreadyValidated);
			Streams::<T>::insert(&event_id, (&sender, current_block));
			Self::deposit_event(Event::ValidatedEvent { event_id });
			Ok(())
		}
	}
	sp_api::decl_runtime_apis! {
		pub trait ExtrinsicDetails{
			fn get_extrinsic_ids(extrinsics: &Vec<Block::Extrinsic>) -> Vec<H256>;
		}
	}
}
