# Validated Streams Pallet
## Overview
The validated streams pallet serve as the final verification pipeline for an event, after the event has been witnessed by two thirds of validators off-chain, validators create an unsigned `validate_event` extrinsic and submit it, the event will finally be included on-chain if it has not been already added.

## Interface
### Dispatchable Functions
* **`validate_event`**: checks if the event has already been added on chain via the `Streams StorageMap`. If so, it raise an `AlreadyValidated` event. If not, it inserts the event and the current block into the storage and raise a `ValidatedEvent` event.
### Helper methods
in order to make it easier for other pallets to access the Streams StorageMap we created the following methods:
* **`get_all_events`**: get all events from the Streams StorageMap.
* **`get_block_events`**: get all events of a specific block.
* **`verify_event`**:  verify whether an event is valid or not

### Runtime Api
* **`get_extrinsic_ids`**: retreives the event ids for a vector of block extrinsics
* **`create_unsigned_extrinsic`**: given an event id of type `sp_core:H256`, it returns an unsigned `validate_event` extrinsic.
