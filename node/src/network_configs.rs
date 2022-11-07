use local_ip_address::local_ip;
// trait used to make it easier to create networkconfigurations from diffrent sources
pub trait NetworkConfiguration {
	fn get_self_address(&self) -> String;
	fn get_peers_addresses(&self) -> Vec<String>;
}

pub struct LocalDockerNetworkConfiguration {
	pub port: u16,
}
impl NetworkConfiguration for LocalDockerNetworkConfiguration {
	fn get_self_address(&self) -> String {
		format!("{}:{}", local_ip().unwrap().to_string(), &self.port)
	}
	fn get_peers_addresses(&self) -> Vec<String> {
		vec![
			format!("http://172.17.0.2:{}", &self.port),
			format!("http://172.17.0.3:{}", &self.port),
			format!("http://172.17.0.4:{}", &self.port),
			format!("http://172.17.0.5:{}", &self.port),
		]
	}
}
pub struct LocalNetworkConfiguration {
	pub port: u16,
}
impl NetworkConfiguration for LocalNetworkConfiguration {
	fn get_self_address(&self) -> String {
		format!("127.0.0.1:{}", &self.port)
	}
	fn get_peers_addresses(&self) -> Vec<String> {
		vec![
			format!("http://127.0.0.1:{}", &self.port),
			format!("http://127.0.0.1:{}", &self.port + 1),
			format!("http://127.0.0.1:{}", &self.port + 2),
			format!("http://127.0.0.1:{}", &self.port + 3),
		]
	}
}
