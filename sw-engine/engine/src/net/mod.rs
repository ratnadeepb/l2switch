/*
* Copyright 2019 Comcast Cable Communications Management, LLC
*
* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at
*
* http://www.apache.org/licenses/LICENSE-2.0
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*
* SPDX-License-Identifier: Apache-2.0
*/

//! Common network utilities.

mod cidr;
mod ether;
mod ipv4;
mod mac;
mod routing_table;

pub use self::cidr::{Cidr, CidrError, Ipv4Cidr, Ipv6Cidr};
pub use self::ether::EtherHdr;
pub use self::ipv4::Ipv4Hdr;
pub use self::mac::{MacAddr, MacParseError};
pub use self::routing_table::RoutingTable;

use std::net::Ipv4Addr;

pub struct FiveTuple {
	src_mac: MacAddr,
	dst_mac: MacAddr,
	src_ip: Ipv4Addr,
	dst_ip: Ipv4Addr,
	proto: u8,
}

impl FiveTuple {
	pub fn new(ipv4_hdr: Ipv4Hdr, ether_hdr: EtherHdr) -> Self {
		let ehdr = ether_hdr.get();
		let ihdr = ipv4_hdr.get();
		Self {
			src_mac: ehdr.s_addr.addr_bytes.into(),
			dst_mac: ehdr.d_addr.addr_bytes.into(),
			src_ip: ihdr.src_addr.into(),
			dst_ip: ihdr.dst_addr.into(),
			proto: ihdr.next_proto_id,
		}
	}

	pub fn get_d_mac(&self) -> MacAddr {
		self.dst_mac
	}

	pub fn get_d_ip(&self) -> Ipv4Addr {
		self.dst_ip
	}
}
