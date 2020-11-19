/*
 * Created on Thu Nov 12 2020:14:53:10
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	dpdk::Mbuf, info,
	net::{EtherHdr, FiveTuple, Ipv4Hdr},
	PortIdMbuf, FORWARDING_TABLE, PORTMAP,
	PORTS,
};
use crossbeam_queue::ArrayQueue;
use futures::{self, task::LocalSpawnExt};

/// Packet receive function
/// Receive packets from the NIC and place them in the deque mbufs
pub async fn rx_main(
	mut receiver: futures::channel::oneshot::Receiver<()>,
	mbufs: &ArrayQueue<PortIdMbuf>,
) -> () {
	let ports = PORTS.get();

	loop {
		match receiver.try_recv() {
			Ok(_) => break, // if we receive anything over the channel then stop receiving
			Err(futures::channel::oneshot::Canceled) => (),
		}
		// let mut deque = VecDeque::new();
		while !mbufs.is_full() {
			for port in ports {
				let recvd = port.receive(); // receive a batch of 32 packets from
							// push to the end of the VecDeque, pop will happen from the front
				match mbufs.push(PortIdMbuf {
					portid: port.get_portid(),
					buf: recvd,
				}) {
					Ok(()) => {}
					Err(_) => info!("Failed to push pkt"),
				};
			}
		}
	}
	()
}

/// Consumes the packets placed in the deque by rx_main
/// Extract the five tuple from each mbuf
/// Associate the five tuple with a port id
/// Update the routing table
pub async fn process_packets(mbufs: &ArrayQueue<PortIdMbuf>) {
	while !mbufs.is_empty() {
		match mbufs.pop() {
			Some(elem) => {
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

						// process the packet
						tx_to_clients(mbuf);
					}
				}
			}
			None => 
				info!("Since we check that the array is non-empty before popping an mbuf, this branch should not have run!")
		};
	}
}

/// Transmit packets to L3 containers through DPDK rings
/// Consumes the buffer
fn tx_to_clients(mut mbuf: Mbuf) {
	unimplemented!();
}

/// Function for each thread to run
fn work_horse(
	receiver: futures::channel::oneshot::Receiver<()>,
	mbufs: &'static ArrayQueue<PortIdMbuf>,
) {
	// create an executor to run on the local thread only
	let mut executor = futures::executor::LocalPool::new();
	// a task spawner associated to the executor
	let spawner = executor.spawner();
	// create the required futures (green threads)
	let rx_fut = rx_main(receiver, mbufs); // get packets
	let process_pkts_fut = process_packets(mbufs); // process packets
	// spawn the futures
	let rx_fut_handle = spawner.spawn_local_with_handle(rx_fut).unwrap();
	spawner.spawn_local(process_pkts_fut).unwrap();
	// run the executor till rx_fut returns
	// drop everything the moment the rx_main function returns
	executor.run_until(rx_fut_handle);
}
