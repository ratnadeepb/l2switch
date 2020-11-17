/*
 * Created on Thu Nov 12 2020:14:53:10
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	debug,
	dpdk::Mbuf,
	error, ffi, info,
	net::{EtherHdr, Ipv4Hdr},
	PortQueue, ETHER_HDR_SZ, IPV4_HDR_SZ, MBUFS_RECVD, PACKET_READ_SIZE, PORTS,
};
use dpdk_ffi;
use failure::{Fail, Fallible};
use futures;
use std::collections::HashMap;

/// Packet receive function
async fn rx_main(mut receiver: futures::channel::oneshot::Receiver<()>) {
	// let pkts: Vec<Mbuf> = Vec::with_capacity(PACKET_READ_SIZE);
	// let rx_count: usize;
	// let cur_lcore = unsafe { dpdk_ffi::_rte_lcore_id() };
	// let ports: &Vec<PortQueue> = &*(*PORTS);
	let ports = PORTS.get();

	loop {
		match receiver.try_recv() {
			Ok(_) => break, // if we receive anything over the channel then stop receiving
			Err(futures::channel::oneshot::Canceled) => (),
		}
		let mut hashmap = HashMap::new();
		for port in ports {
			let mbufs = port.receive(); // receive a batch of 32 packets from
			hashmap.insert(port.get_portid(), mbufs); // add the batch of packets to the hashmap
		}
		MBUFS_RECVD.with(|tls| tls.set(hashmap)); // store the packets locally in the thread
	}
}

/// get the source IP and MAC address of each packet
/// associate with the port on which it was received
/// can be used for security purposes later
async fn get_port_ip_mac_mapping() {
	MBUFS_RECVD.with(|hashmap| {
		let portmap: HashMap<u16, Vec<Mbuf>> = HashMap::new();
		for (pnum, mbufs) in hashmap.take().iter_mut() {
			if !mbufs.is_empty() {
				for mbuf in mbufs.iter_mut() {
					let ether_hdr = EtherHdr::from_mbuf(mbuf);
					let ipv4_hdr = Ipv4Hdr::from_mbuf(mbuf);
				}
			}
		}
	})
}

/// Function for each thread to run
fn work_horse() {
	let s = futures::executor::LocalPool::new();
	let rx = async {};
}
