//! Configurations used by the Validated Streams node

use libp2p::Multiaddr;
use local_ip_address::local_ip;

/// Network configuration for the local testnet
// TODO: Make configurable or use sc_config::network
pub struct DebugLocalNetworkConfiguration {}
impl DebugLocalNetworkConfiguration {
	/*
	fn get_self_address(&self) -> String {
		format!("{}:{}", local_ip().unwrap(), &self.port)
	}
	fn get_peers_addresses(&self) -> Vec<String> {
		vec![
			format!("http://172.19.0.2:{}", &self.port),
			format!("http://172.19.0.3:{}", &self.port),
			format!("http://172.19.0.4:{}", &self.port),
			format!("http://172.19.0.5:{}", &self.port),
		]
	}
	*/
	/// Returns the multiaddr gossip should listen at
	pub fn self_multiaddr() -> Multiaddr {
		format!("/ip4/{}/tcp/10000", local_ip().expect("failed getting local ip"))
			.parse()
			.expect("failed getting self multi address")
	}
	/// Returns all the multiaddrs of peers (filters validators by removing self)
	pub fn peers_multiaddrs(self_addr: Multiaddr) -> Vec<Multiaddr> {
		let validators_multiaddrs = vec![
			"/ip4/172.19.0.2/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.3/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.4/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.5/tcp/10000".parse().expect("Erroneous Multiaddr"),
		];
		validators_multiaddrs.into_iter().filter(|peer| *peer != self_addr).collect()
	}
}
