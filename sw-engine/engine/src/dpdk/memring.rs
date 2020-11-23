/*
 * Created on Thu Oct 29 2020:10:29:14
 * Created by Ratnadeep Bhattacharya
 */

use super::SocketId;
use crate::{
	debug,
	dpdk::{DpdkError, Mbuf},
	ffi::{AsStr, ToCString, ToResult},
	info,
};
use dpdk_ffi;
use failure::{format_err, Fallible};
use std::{collections::HashMap, ffi::c_void, fmt, os::raw, ptr::NonNull, sync::Arc};

const NO_FLAGS: u8 = 0;

/// The RingType is whether message is being sent from engine to container or from contianer to engine
pub enum RingType {
	RX,
	TX,
}

/// A ring is intended to communicate between two DPDK processes by sending/receiving `Mbuf`.
/// For best performance, each socket should have a dedicated `Mempool`.
pub struct Ring {
	client_id: u16,
	rtype: RingType,
	raw: NonNull<dpdk_ffi::rte_ring>,
}

// allows to create a ring before a client id is available
// most likely will not be required
impl Default for Ring {
	fn default() -> Self {
		Self {
			client_id: 0,
			rtype: RingType::TX,
			raw: NonNull::dangling(),
		}
	}
}

impl Ring {
	/// Return a Ring created from a pointer if the pointer is not null
	pub fn from_ptr(client_id: u16, rtype: RingType, r: *mut dpdk_ffi::rte_ring) -> Option<Self> {
		if let Some(raw) = NonNull::new(r) {
			Some(Self {
				client_id,
				rtype,
				raw,
			})
		} else {
			None
		}
	}

	/// Creates a new `Ring` for `Mbuf`.
	///
	/// `capacity` is the maximum number of `Mbuf` the `Mempool` can hold.
	/// The optimum size (in terms of memory usage) is when n is a power
	/// of two minus one.
	///
	/// `socket_id` is the socket where the memory should be allocated. The
	/// value can be `SocketId::ANY` if there is no constraint.
	///
	/// # Errors
	///
	/// If allocation fails, then `DpdkError` is returned.
	pub fn new(
		client_id: u16,
		rtype: RingType,
		name: String,
		capacity: usize,
		socket_id: SocketId,
	) -> Fallible<Self> {
		let raw = unsafe {
			dpdk_ffi::rte_ring_create(
				name.clone().to_cstring().as_ptr(),
				capacity as raw::c_uint,
				socket_id.raw(),
				NO_FLAGS as raw::c_uint,
			)
			.to_result(|_| DpdkError::new())?
		};
		info!("Created ring {}", name);
		Ok(Self {
			client_id,
			rtype,
			raw,
		})
	}

	/// Get the name to lookup with
	#[inline]
	pub fn name(&self) -> String {
		let mut name: &str;
		match self.rtype {
			RingType::RX => name = "RX-".into(),
			RingType::TX => name = "TX-".into(),
		}
		let id = format!("{}", self.client_id);
		format!("{}{}", name, id)
		// name
	}

	/// Enqueue a single packet onto the ring
	pub fn enqueue(&mut self, pkt: Mbuf) -> Fallible<()> {
		// match unsafe { dpdk_ffi::_rte_ring_enqueue(self.raw_mut(), pkt.into_ptr() as *mut c_void) }
		// {
		// 	0 => true,
		// 	_ => false,
		// }
		unsafe {
			dpdk_ffi::_rte_ring_enqueue(self.raw_mut(), pkt.into_ptr() as *mut c_void)
				.to_result(|_| DpdkError::new())?
		};
		Ok(())
	}

	/// Dequeue a single packet from the ring
	pub fn dequeue(&mut self, pkt: &mut Mbuf) -> Fallible<()> {
		unsafe {
			dpdk_ffi::_rte_ring_dequeue(
				self.raw_mut(),
				&mut (pkt.get_ptr() as *mut _ as *mut c_void),
			)
			.to_result(|_| DpdkError::new())?
		};
		Ok(())
	}

	/// Returns the raw struct needed for FFI calls.
	#[inline]
	pub fn raw(&self) -> &dpdk_ffi::rte_ring {
		unsafe { self.raw.as_ref() }
	}

	/// Returns the raw struct needed for FFI calls.
	#[inline]
	pub fn raw_mut(&mut self) -> &mut dpdk_ffi::rte_ring {
		unsafe { self.raw.as_mut() }
	}

	// Returns the name of the `Mempool`.
	// #[inline]
	// pub fn name(&self) -> &str {
	// 	self.raw().name[..].as_str()
	// }
}

impl fmt::Debug for Ring {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let raw = self.raw();
		unsafe {
			f.debug_struct(&self.name()[..])
				.field("capacity", &raw.capacity)
				.field("flags", &format_args!("{:#x}", raw.flags))
				.finish()
		}
	}
}

impl Drop for Ring {
	fn drop(&mut self) {
		debug!("freeing {}.", self.name());
		unsafe {
			dpdk_ffi::rte_ring_free(self.raw_mut());
		}
	}
}

/// The engine and client communicate with each other through
/// a transmit and a receive Ring
/// These two Rings together form a channel
pub struct Channel {
	pub(crate) tx_q: Ring,
	pub(crate) rx_q: Ring,
}

impl Channel {
	/// New Channel
	pub fn new(tx_q: Ring, rx_q: Ring) -> Self {
		Self { tx_q, rx_q }
	}

	/// Send a packet
	pub fn send(&mut self, pkt: Mbuf) -> Fallible<()> {
		self.tx_q.enqueue(pkt)
	}

	/// Receive a packet
	pub fn receive(&mut self, pkt: &mut Mbuf) -> Fallible<()> {
		self.rx_q.dequeue(pkt)
	}
}

impl Drop for Channel {
	fn drop(&mut self) {}
}

/// Ring to container client maps
/// This structure owns all the rings of the engine
pub struct EngineRingMap {
	ring_map: HashMap<u16, Channel>,
}

impl EngineRingMap {
	/// Send a packet to a container
	pub fn send(&mut self, key: u16, pkt: Mbuf) -> Fallible<()> {
		match self.ring_map.get_mut(&key) {
			Some(ch) => ch.send(pkt),
			None => Err(format_err!("Failed to send packet")),
		}
	}

	/// Receive a packet from a container
	pub fn receive(&mut self, key: u16, pkt: &mut Mbuf) -> Fallible<()> {
		match self.ring_map.get_mut(&key) {
			Some(ch) => ch.receive(pkt),
			None => Err(format_err!("Failed to receive packet")),
		}
	}
}
