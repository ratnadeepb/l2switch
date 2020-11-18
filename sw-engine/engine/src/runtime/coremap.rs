/*
 * Created on Thu Nov 12 2020:14:53:10
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	debug,
	dpdk::Mbuf,
	error, ffi, info,
	net::{EtherHdr, FiveTuple, Ipv4Hdr},
	PortIdMbuf, PortQueue, ETHER_HDR_SZ, FORWARDING_TABLE, IPV4_HDR_SZ, PACKET_READ_SIZE,
	PORTMAP, PORTS,
};
use dpdk_ffi;
use failure::{Fail, Fallible};
use futures;
use std::collections::VecDeque;

/// Packet receive function
/// Receive packets from the NIC and place them in the deque mbufs
async fn rx_main(
	mut receiver: futures::channel::oneshot::Receiver<()>,
	mbufs: &mut VecDeque<PortIdMbuf>,
) {
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
		let mut deque = VecDeque::new();
		for port in ports {
			let mbufs = port.receive(); // receive a batch of 32 packets from
							// push to the end of the VecDeque, pop will happen from the front
			deque.push_back(PortIdMbuf {
				portid: port.get_portid(),
				buf: mbufs,
			});
		}
		mbufs.append(&mut deque);
		// unsafe { MBUFS.append(&mut deque) };
		// MBUFS.with(|tls| tls.set(deque)); // store the packets locally in the thread
	}
}

/// Consumes the packets placed in the deque by rx_main
/// Extract the five tuple from each mbuf
/// Associate the five tuple with a port id
/// Update the routing table
async fn process_packets(mbufs: &mut VecDeque<PortIdMbuf>) {
	// let mut deque: VecDeque<PortIdMbuf> = VecDeque::new();
	// let deque = unsafe { MBUFS.drain(..).collect::<VecDeque<_>>() };
	// MBUFS.with(|list| {
	// 	// the take() consumes the memory leaving the RefCell in default state
	// 	deque = list.take();
	// }); // MBUFS is released
	// the loop consumes the mbufs stored in the VecDeque
	// iterating over a VecDeque goes front to back

	let deque = mbufs.drain(..).collect::<VecDeque<_>>();
	for elem in deque {
		let pnum = elem.portid;
		let mbufs = elem.buf;
		if !mbufs.is_empty() {
			for mut mbuf in mbufs {
				let ether_hdr = EtherHdr::from_mbuf(&mut mbuf);
				let ipv4_hdr = Ipv4Hdr::from_mbuf(&mut mbuf);
				let five_tuple = FiveTuple::new(ipv4_hdr, ether_hdr);
				let d_mac = five_tuple.get_d_mac();
				let d_ip = five_tuple.get_d_ip();
				// Add five tuple to the portmap
				let portmap = PORTMAP.get();
				portmap.insert(pnum, five_tuple);

				// update forwarding table
				let forwarding_table = FORWARDING_TABLE.get();
				forwarding_table.add(d_mac, d_ip);
			}
		}
	}
}

/// Transmit packets to L3 containers through DPDK rings
async fn tx_main(mut receiver: futures::channel::oneshot::Receiver<()>) {
	loop {
		match receiver.try_recv() {
			Ok(_) => break,
			Err(futures::channel::oneshot::Canceled) => (),
		}
		unimplemented!();
	}
}
/// Function for each thread to run
fn work_horse() {
	let s = futures::executor::LocalPool::new();
	let rx = async {};
}
