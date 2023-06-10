//! Configurations used by the Validated Streams node

use libp2p::{core::multiaddr::Protocol, Multiaddr};

use std::{fmt, net::SocketAddr, str::FromStr};

/// Network configuration for the Validated Streams node (type alias for now, might change under
/// future refactorings)
pub type ValidatedStreamsNetworkConfiguration = ValidatedStreamsNetworkParams;

/// Network configuration for the Validated Streams node
#[derive(Debug, Clone, clap::Args)]
pub struct ValidatedStreamsNetworkParams {
	/// Address to listen to grpc calls for the current validated streams node
	/// Ideally, only open to the local machine
	#[clap(long, default_value = "127.0.0.1:5555")]
	pub grpc_addr: Vec<SocketAddr>,
	/// port used for libp2p gossipsub in the validated streams code
	/// note that the same addresses will be used as for the substrate network
	#[clap(long, default_value_t = PortOrOffset::Offset(10))]
	pub gossip_port: PortOrOffset,
	/// override for the bootnodes used for gossiping
	#[clap(long)]
	pub gossip_bootnodes: Vec<Multiaddr>,
}

/// A specific port number or an offset from the base port number. Used to subtly adjust an address
/// so as to not conflict.
#[derive(Debug, Copy, Clone)]
pub enum PortOrOffset {
	/// A specific port number.
	Port(u16),
	/// An offset from the original port number
	Offset(i16),
}

impl PortOrOffset {
	/// apply the PortOrOffset to a port number
	pub fn adjust_port(&self, port: u16) -> u16 {
		match self {
			Self::Port(port) => *port,
			Self::Offset(offset) => port.saturating_add_signed(*offset),
		}
	}
	/// apply the PortOrOffset to a Multiaddr
	pub fn adjust_multiaddr(&self, mut addr: Multiaddr) -> Multiaddr {
		let mut protocols = vec![];

		loop {
			match addr.pop() {
				Some(Protocol::Udp(port)) => {
					protocols.push(Protocol::Udp(self.adjust_port(port)));
					break
				},
				Some(Protocol::Tcp(port)) => {
					protocols.push(Protocol::Tcp(self.adjust_port(port)));
					break
				},
				Some(protocol) => protocols.push(protocol),
				None => break,
			}
		}

		protocols.into_iter().for_each(|protocol| addr.push(protocol));

		addr
	}
}

impl fmt::Display for PortOrOffset {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::Port(port) => fmt::Display::fmt(port, f),
			Self::Offset(offset) => {
				if *offset >= 0 {
					write!(f, "+{offset}")
				} else {
					write!(f, "-{}", -offset)
				}
			},
		}
	}
}

impl FromStr for PortOrOffset {
	type Err = <u16 as FromStr>::Err;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut chars = s.chars();
		Ok(match chars.next() {
			Some('+') => Self::Offset(i16::from_str(chars.as_str())?),
			Some('-') => Self::Offset(-i16::from_str(chars.as_str())?),
			_ => Self::Port(u16::from_str(s)?),
		})
	}
}
