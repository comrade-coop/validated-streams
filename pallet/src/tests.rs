use crate as pallet_validated_streams;
use frame_support::{
	assert_err, assert_ok,
	traits::{ConstU16, ConstU64},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

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
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
/// dispatch an event to the streams StorageMap and check whether an en event has been raised
/// then dispatch the same event to verify Error handling since duplicates are not allowed
#[test]
fn it_adds_event() {
	new_test_ext().execute_with(|| {
		// Go past genesis block so events get deposited
		System::set_block_number(1);
		let event_id = H256::repeat_byte(0);
		assert_eq!(ValidatedStreams::verify_event(event_id), false);
		// Dispatch an extrinsic
		// signature should not matter since it should pass through validate_unsigned.
		assert_ok!(ValidatedStreams::validate_event(Origin::none(), event_id));
		assert_eq!(ValidatedStreams::get_all_events(), vec![event_id]);
		assert_eq!(ValidatedStreams::verify_event(event_id), true);
		System::assert_last_event(
			pallet_validated_streams::Event::ValidatedEvent { event_id }.into(),
		);
		//double check the first block events
		assert_eq!(ValidatedStreams::get_block_events(1), vec![event_id]);
		//dispatch an extrinsic with an already validated event
		assert_err!(
			ValidatedStreams::validate_event(Origin::root(), event_id),
			pallet_validated_streams::Error::<Test>::AlreadyValidated
		);
	})
}
