/*
 * Created on Thu Nov 12 2020:14:53:10
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	dpdk::{Mbuf, EngineRingMap, Ring, RingType, SocketId, Channel}, info,
	net::{EtherHdr, FiveTuple, Ipv4Hdr},
	PortIdMbuf, FORWARDING_TABLE, PORTMAP,
	PORTS, dockerlib::SOCKET, PACKET_READ_SIZE,
};
use serde_json;
use crossbeam_queue::ArrayQueue;
use std::{result, collections::HashMap, cell::Cell, time::Duration};
use futures::{self, task::LocalSpawnExt};
use failure::{Fail, Fallible, format_err};
use async_std::task;
use chashmap::CHashMap;

/// Check for messages every 10 ms
const TIMER_VAL: u64 = 10;

#[derive(PartialEq)]
enum ClientStatus {
	STARTING,
	READY,
}

/// The engine that forms the core of the L2 forwarding plane
pub struct Engine {
	statusmap: CHashMap<u16, ClientStatus>, // maintain status of the clients
	ringmap: EngineRingMap, // map for data plane Rings for clients registered
	socket: zmq::Socket, // open a server socket to let clients register
	new_msg: Cell<Option<(u16, u64)>>, // new message over the zmq socket
}

type Result<Engine> = result::Result<Engine, zmq::Error>;

impl Engine {
	pub fn new() -> Result<Self> {
		let statusmap = CHashMap::new();
		let ringmap = EngineRingMap::new();
		let context = zmq::Context::new();
		match context.socket(zmq::REP) {
			Ok(socket) => {
				socket.connect(SOCKET)?; // return error if connection failed
				Ok(Self { statusmap, ringmap, socket, new_msg: Cell::new(None) })
			},
			Err(err) => Err(err),
		}
	}

	/// Get events from socket
	fn get_msg(&self) -> Fallible<()> {
		let timeout = 2; // timeout if there are no messages in 2 ms
		match self.socket.poll(zmq::POLLIN, timeout) {
			Ok(1) => {
				let mut msg = zmq::Message::new();
				self.socket.recv(&mut msg, zmq::DONTWAIT)?; // non-blocking receive
				if let Some(smsg) = msg.as_str() {
					let data: serde_json::Value = serde_json::from_str(smsg)?;
					if let Some(id) = data["id"].as_u64() {
						if let Some(m) = data["msg"].as_u64() {
							// New message has been received and set in the engine
							self.new_msg.set(Some((id as u16, m)));
							Ok(())
						}
						else {
							return Err(format_err!("Unknown Container Message"));
						}
					} else {
							return Err(format_err!("Unknown ID Format"));
						}
				} else {
					return Err(format_err!("Unknown Message Format"));
				}
			},
			Ok(_) => Ok(()), // no messages as of now
			Err(e) => return Err(format_err!("{}", e.to_string())),
		}
	}

	/// Register a new client or change its status
	fn set_client_status(&self, id: u16, m: u64) -> Fallible<()> {
		match m {
			0 => {
				let rx_q = Ring::new(
						id as u16,
						RingType::RX,
						format!("RX-{}", id),
						PACKET_READ_SIZE,
						SocketId::current()
						)?;
				let tx_q = Ring::new(
						id as u16,
						RingType::TX,
						format!("TX-{}", id),
						PACKET_READ_SIZE,
						SocketId::current()
						)?;
				// NOTE: if send fails then the client never gets ready
				// send an error back
				self.socket.send("1", 0)?;
				// NOTE: no error means message was successfully sent
				// still need to check if message was received successfully on client side
				// but engine can move on
				self.ringmap.ring_map.insert(id as u16, Channel { tx_q, rx_q});
				self.statusmap.insert(id as u16, ClientStatus::STARTING);
			},
			1 => {
				let mut v = self.statusmap.get_mut(&(id as u16)).ok_or_else(|| format_err!("Failed to get client"))?;
				*v = ClientStatus::READY;
			},
			2 => {
				self.statusmap.remove(&(id as u16)).ok_or_else(|| format_err!("Failed to remove client"))?;
				self.ringmap.ring_map.remove(&(id as u16)).ok_or_else(|| format_err!("Failed to remove client"))?;
			},
			_ => return Err(format_err!("Unknown client status")),
		}
		Ok(())
	}

	/// Check for messages
	/// set client status if there are messages
	/// every TIMER_VAL period
	async fn check_n_set_client_status(&self) -> () {
		task::sleep(Duration::from_micros(TIMER_VAL)).await;
		if let Some(val) = self.new_msg.get() {
			self.set_client_status(val.0, val.1).ok(); // NOTE: discards the error
			self.new_msg.set(None);
		}
		()
	}

	/// Send packet to a client
	fn send(&self, key: u16, pkt: Mbuf) -> Fallible<()> {
		self.ringmap.send(key, pkt)
	}

	/// Receive a packet from a client
	fn receive(&self, key: u16, pkt: &mut Mbuf) -> Fallible<()> {
		self.ringmap.receive(key, pkt)
	}

	/// Packet receive function
	/// Receive packets from the NIC and place them in the deque mbufs
	async fn rx_main(
		&self, 
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
	async fn process_packets(&self, mbufs: &ArrayQueue<PortIdMbuf>) {
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
							self.tx_to_clients(mbuf);
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
	fn tx_to_clients(&self, mut mbuf: Mbuf) {
		unimplemented!();
	}

	/// Function for each thread to run
	pub fn work_horse(
		&'static self,
		receiver: futures::channel::oneshot::Receiver<()>,
		mbufs: &'static ArrayQueue<PortIdMbuf>,
	) -> Fallible<()> {
		// create an executor to run on the local thread only
		let mut executor = futures::executor::LocalPool::new();
		// a task spawner associated to the executor
		let spawner = executor.spawner();
		// create the required futures (green threads)
		let reg_fut = self.check_n_set_client_status(); // check for new clients
		let rx_fut = self.rx_main(receiver, mbufs); // get packets
		let process_pkts_fut = self.process_packets(mbufs); // process packets
		// spawn the futures
		let rx_fut_handle = spawner.spawn_local_with_handle(rx_fut)?;
		spawner.spawn_local(process_pkts_fut)?;
		spawner.spawn_local(reg_fut)?;
		// run the executor till rx_fut returns
		// drop everything the moment the rx_main function returns
		executor.run_until(rx_fut_handle);
		Ok(())
	}
}