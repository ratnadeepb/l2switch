/*
 * Created on Tue Nov 17 2020:00:50:06
 * Created by Ratnadeep Bhattacharya
 */

use crate::dpdk::Mbuf;
use dpdk_ffi;

pub struct Ipv4Hdr(dpdk_ffi::rte_ipv4_hdr);

impl Ipv4Hdr {
	pub fn from_mbuf(buf: &mut Mbuf) -> Self {
		Self(unsafe { *(dpdk_ffi::_pkt_ipv4_hdr(buf.raw_mut()) as *mut dpdk_ffi::rte_ipv4_hdr) })
	}

	pub fn get(self) -> dpdk_ffi::rte_ipv4_hdr {
		self.0
	}
}
