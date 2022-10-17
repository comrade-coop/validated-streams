#![cfg_attr(not(feature = "std"), no_std)]
// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_support::{debug, dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_aura::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ValidatedStream { StreamId: T::Hash },
	}
	#[pallet::error]
	pub enum Error<T> {
		AlreadyValidated,
	}
	#[pallet::storage]
	pub(super) type Streams<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, (T::AccountId, T::BlockNumber)>;

	//// Dispatchable functions allow users to interact with the pallet and invoke state changes.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		pub fn validate_stream(origin: OriginFor<T>, stream: T::Hash) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			let sender = ensure_signed(origin)?;
			//verify if tx already exists in the tx pool if not primary
			// Verify that the specified claim has not already been stored.
			//pallet_aura::pallet::Pallet::<T>::;

			//let primary = T::Primary;
			ensure!(!Streams::<T>::contains_key(&stream), Error::<T>::AlreadyValidated);

			// Get the block number from the FRAME System pallet.
			let current_block = <frame_system::Pallet<T>>::block_number();

			// Store the claim with the sender and block number.
			Streams::<T>::insert(&stream, (&sender, current_block));

			// Emit an event that the claim was created.
			Self::deposit_event(Event::ValidatedStream { StreamId: stream });

			Ok(())
		}
	}
}
