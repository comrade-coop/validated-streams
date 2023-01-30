use libp2p::Multiaddr;
use local_ip_address::local_ip;
// trait used to make it easier to create networkconfigurations from diffrent sources
pub trait NetworkConfiguration {
	fn get_self_address(&self) -> String;
	fn get_peers_addresses(&self) -> Vec<String>;
}

pub struct LocalNetworkConfiguration {
	pub port: u16,
}
impl NetworkConfiguration for LocalNetworkConfiguration {
	fn get_self_address(&self) -> String {
		format!("{}:{}", local_ip().unwrap().to_string(), &self.port)
	}
	fn get_peers_addresses(&self) -> Vec<String> {
		vec![
			format!("http://172.19.0.2:{}", &self.port),
			format!("http://172.19.0.3:{}", &self.port),
			format!("http://172.19.0.4:{}", &self.port),
			format!("http://172.19.0.5:{}", &self.port),
		]
	}
}

impl LocalNetworkConfiguration {
	pub fn self_multi_addr() -> Multiaddr {
		format!("/ip4/{}/tcp/10000", local_ip().expect("failed getting local ip").to_string())
			.parse()
			.expect("failed getting self multi address")
	}
	pub fn validators_multiaddrs() -> Vec<Multiaddr> {
		vec![
			"/ip4/172.19.0.2/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.3/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.4/tcp/10000".parse().expect("Erroneous Multiaddr"),
			"/ip4/172.19.0.5/tcp/10000".parse().expect("Erroneous Multiaddr"),
		]
	}
	pub fn peers_multiaddrs(self_addr: Multiaddr) -> Vec<Multiaddr> {
		LocalNetworkConfiguration::validators_multiaddrs()
			.into_iter()
			.filter(|peer| *peer != self_addr)
			.collect()
	}
}
