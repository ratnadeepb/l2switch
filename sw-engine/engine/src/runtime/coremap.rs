/*
 * Created on Thu Nov 12 2020:14:53:10
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	debug,
	dpdk::Mbuf,
	error, ffi, info,
	net::{EtherHdr, FiveTuple, Ipv4Hdr},
	PortQueue, ETHER_HDR_SZ, FORWARDING_TABLE, IPV4_HDR_SZ, MBUFS_RECVD, PACKET_READ_SIZE, PORTMAP,
	PORTS,
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

/// Extract the five tuple from each mbuf
/// Associate the five tuple with a port id
/// Update the routing table
async fn update_mappings() {
	MBUFS_RECVD.with(|hashmap| {
		// the take() consumes the memory leaving the hashmap in default state
		for (pnum, mbufs) in hashmap.take().iter_mut() {
			if !mbufs.is_empty() {
				for mbuf in mbufs.iter_mut() {
					let ether_hdr = EtherHdr::from_mbuf(mbuf);
					let ipv4_hdr = Ipv4Hdr::from_mbuf(mbuf);
					let five_tuple = FiveTuple::new(ipv4_hdr, ether_hdr);
					let d_mac = five_tuple.get_d_mac();
					let d_ip = five_tuple.get_d_ip();
					// Add five tuple to the portmap
					let portmap = PORTMAP.get();
					portmap.insert(*pnum, five_tuple);

					// update forwarding table
					let forwarding_table = FORWARDING_TABLE.get();
					forwarding_table.add(d_mac, d_ip);
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
