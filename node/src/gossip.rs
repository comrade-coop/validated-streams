use node_runtime::Block;
use sc_network::PeerId;
use sc_network_gossip::Validator;
struct GossipEventValidator {}

impl Validator<Block> for GossipEventValidator {
	fn validate(
		&self,
		context: &mut dyn sc_network_gossip::ValidatorContext<Block>,
		sender: &PeerId,
		data: &[u8],
	) -> sc_network_gossip::ValidationResult<<Block as sp_api::BlockT>::Hash> {
		println!("Received Event with data {:?}", data);
		println!("Origin::{:?}", sender);
		sc_network_gossip::ValidationResult::Discard
	}
}
