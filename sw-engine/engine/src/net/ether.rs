/*
 * Created on Tue Nov 17 2020:01:07:12
 * Created by Ratnadeep Bhattacharya
 */

use crate::dpdk::Mbuf;
use dpdk_ffi;

pub struct EtherHdr(dpdk_ffi::rte_ether_hdr);

impl EtherHdr {
	pub fn from_mbuf(buf: &mut Mbuf) -> Self {
		Self(unsafe { *(dpdk_ffi::_pkt_ether_hdr(buf.raw_mut()) as *mut dpdk_ffi::rte_ether_hdr) })
	}
}
