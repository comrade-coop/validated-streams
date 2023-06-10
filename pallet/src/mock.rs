use crate as pallet_validated_streams;
use frame_support::{
	once_cell::sync::Lazy,
	traits::{ConstU16, ConstU32, ConstU64},
	BoundedVec,
};
use frame_system as system;
use sc_keystore::LocalKeystore;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{sr25519::Public, ByteArray, H256};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::sync::Mutex;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub static KEYSTORE: Lazy<LocalKeystore> = Lazy::new(LocalKeystore::in_memory);
pub static PAIRS: Mutex<Vec<Public>> = Mutex::new(Vec::new());
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
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_validated_streams::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_validated_streams::weights::SubstrateWeight<Test>;

	type VSAuthorityId = AuraId;

	type VSMaxAuthorities = ConstU32<32>;

	fn authorities() -> BoundedVec<Self::VSAuthorityId, Self::VSMaxAuthorities> {
		let pairs = PAIRS.lock().unwrap();
		// let key = KEYSTORE.keys(AURA).unwrap().get(i).unwrap().clone();
		pairs
			.iter()
			.map(|pair| AuraId::from_slice(pair.as_slice()).unwrap())
			.collect::<Vec<_>>()
			.try_into()
			.unwrap()
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
	pub use frame_support::BoundedBTreeMap;
	pub use sp_core::{crypto::CryptoTypePublicPair, sr25519::Signature};
	pub use sp_keystore::SyncCryptoStore;
	pub use sp_runtime::key_types::AURA;
	use std::collections::BTreeMap;

	pub fn initialize() {
		let mut pairs = PAIRS.lock().unwrap();
		for _ in pairs.len()..4 {
			pairs.push(KEYSTORE.sr25519_generate_new(AURA, None).unwrap());
		}
	}
	pub fn proofs(
		event_id: &H256,
	) -> BoundedBTreeMap<Public, Signature, <Test as Config>::VSMaxAuthorities> {
		let pairs = PAIRS.lock().unwrap();
		pairs
			.iter()
			.map(|key| {
				let signature = KEYSTORE
					.sign_with(AURA, &CryptoTypePublicPair::from(key), event_id.as_bytes())
					.unwrap()
					.unwrap();
				(*key, signature.as_slice().try_into().unwrap())
			})
			.collect::<BTreeMap<_, _>>()
			.try_into()
			.unwrap()
	}
}
