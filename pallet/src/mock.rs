use crate as pallet_validated_streams;
use frame_support::{
	once_cell::sync::Lazy,
	traits::{ConstU16, ConstU32, ConstU64},
	BoundedVec,
};
use frame_system as system;
use sc_keystore::LocalKeystore;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::CryptoTypePublicPair, ByteArray, H256};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::sync::Mutex;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub static KEYSTORE: Lazy<LocalKeystore> = Lazy::new(|| LocalKeystore::in_memory());
pub static PAIRS: Mutex<Vec<CryptoTypePublicPair>> = Mutex::new(Vec::new());
// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
pub enum Test where
Block = Block,
NodeBlock = Block,
UncheckedExtrinsic = UncheckedExtrinsic,
{
	System: frame_system,
	ValidatedStreams: pallet_validated_streams,
}
);

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
}

impl pallet_validated_streams::Config for Test {
	type Event = Event;

	type SignatureLength = ConstU32<64>;

	type VSAuthorityId = AuraId;

	type VSMaxAuthorities = ConstU32<32>;

	fn authorities() -> frame_support::BoundedVec<Self::VSAuthorityId, Self::VSMaxAuthorities> {
		let mut authorities = vec![];
		for i in 0..4 {
			// let key = KEYSTORE.keys(AURA).unwrap().get(i).unwrap().clone();
			let id =
				AuraId::from_slice(PAIRS.lock().unwrap().get(i).unwrap().1.as_slice()).unwrap();
			authorities.push(id);
		}
		BoundedVec::truncate_from(authorities)
	}
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
#[cfg(feature = "on-chain-proofs")]
pub mod onchain_mod {
	use crate::mock::*;
	pub use crate::Config;
	pub use frame_support::{BoundedBTreeMap, BoundedVec};
	pub use sp_core::sr25519::Public;
	pub use sp_keystore::SyncCryptoStore;
	pub use sp_runtime::key_types::AURA;

	pub fn initialize() -> Vec<Public> {
		let mut keys = Vec::new();
		for _ in 0..4 {
			let key = KEYSTORE.sr25519_generate_new(AURA, None).unwrap();
			keys.push(key);
			let pair = CryptoTypePublicPair::from(key);
			PAIRS.lock().unwrap().push(pair.clone());
		}
		keys
	}
	pub fn proofs(
		event_id: &H256,
		keys: &Vec<Public>,
	) -> BoundedBTreeMap<
		Public,
		BoundedVec<u8, <Test as Config>::SignatureLength>,
		<Test as Config>::VSMaxAuthorities,
	> {
		let mut proofs = BoundedBTreeMap::new();
		for i in 0..4 {
			let signature = KEYSTORE
				.sign_with(AURA, &PAIRS.lock().unwrap().get(i).unwrap(), event_id.as_bytes())
				.unwrap()
				.unwrap();
			proofs
				.try_insert(
					keys.get(i).unwrap().clone(),
					BoundedVec::truncate_from(signature.to_vec()),
				)
				.unwrap();
		}
		proofs
	}
}
