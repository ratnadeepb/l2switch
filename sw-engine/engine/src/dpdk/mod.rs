/*
 * Created on Wed Oct 28 2020:17:48:56
 * Created by Ratnadeep Bhattacharya
 */

mod mbuf;
mod mempool;
mod memring;
mod port;

pub use mbuf::*;
pub use mempool::*;
pub use memring::*;
pub use port::*;

use crate::net::MacAddr;
use crate::{debug, ffi::*};
use dpdk_ffi;
use failure::{Fail, Fallible};
use libc;
use std::{cell::Cell, fmt, mem, os::raw};

/// An error generated in `libdpdk`.
///
/// When an FFI call fails, the `errno` is translated into `DpdkError`
#[derive(Debug, Fail)]
#[fail(display = "{}", _0)]
pub(crate) struct DpdkError(String);

impl DpdkError {
	/// Returns the `DpdkError` for the most recent failure on the current
	/// thread
	#[inline]
	pub(crate) fn new() -> Self {
		DpdkError::from_errno(-1)
	}

	/// Returns the `DpdkError` for a specific `errno`
	fn from_errno(errno: raw::c_int) -> Self {
		let errno = if errno == -1 {
			unsafe { dpdk_ffi::_rte_errno() }
		} else {
			-errno
		};
		DpdkError(unsafe { dpdk_ffi::rte_strerror(errno).as_str().into() })
	}
}

/// An opaque identifier for a physical CPU socket.
///
/// A socket is also known as a NUMA node. On a multi-socket system, for best
/// performance, ensure that the cores and memory used for packet processing
/// are in the same socket as the network interface card
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct SocketId(raw::c_int);

impl SocketId {
	/// A socket id for any NUMA node
	pub const ANY: Self = SocketId(-1);

	/// Returns the ID of the socket the current core is on
	#[inline]
	pub fn current() -> Self {
		unsafe { SocketId(dpdk_ffi::rte_socket_id() as raw::c_int) }
	}

	/// Returns all the socket IDs detected on the system.
	#[inline]
	pub fn all() -> Vec<SocketId> {
		unsafe {
			(0..dpdk_ffi::rte_socket_count())
				.map(|idx| dpdk_ffi::rte_socket_id_by_idx(idx))
				.filter(|&sid| sid != -1)
				.map(SocketId)
				.collect::<Vec<_>>()
		}
	}

	/// Returns the raw value needed for FFI calls.
	#[allow(clippy::trivially_copy_pass_by_ref)]
	#[inline]
	pub(crate) fn raw(&self) -> raw::c_int {
		self.0
	}
}

impl fmt::Debug for SocketId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "socket{}", self.0)
	}
}

/// An opaque identifier for a physical CPU core.
#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CoreId(usize);

impl CoreId {
	/// Any lcore to indicate that no thread affinity is set
	pub const ANY: Self = CoreId(std::usize::MAX);

	/// Creates a new CoreId from the numeric ID assigned to the core
	/// by the system.
	#[inline]
	pub(crate) fn new(i: usize) -> CoreId {
		CoreId(i)
	}
	/// Returns the ID of the current core.
	#[inline]
	pub fn current() -> CoreId {
		CURRENT_CORE_ID.with(|tls| tls.get())
	}

	/// Returns the ID of the socket the core is on.
	#[allow(clippy::trivially_copy_pass_by_ref)]
	#[inline]
	pub fn socket_id(&self) -> SocketId {
		unsafe { SocketId(dpdk_ffi::numa_node_of_cpu(self.0 as raw::c_int)) }
	}

	/// Returns the raw value.
	#[allow(clippy::trivially_copy_pass_by_ref)]
	#[inline]
	pub(crate) fn raw(&self) -> usize {
		self.0
	}

	/// Sets the current thread's affinity to this core.
	#[allow(clippy::trivially_copy_pass_by_ref)]
	#[inline]
	pub(crate) fn set_thread_affinity(&self) -> Fallible<()> {
		unsafe {
			// the two types that represent `cpu_set` have identical layout,
			// hence it is safe to transmute between them
			let mut set: libc::cpu_set_t = mem::zeroed();
			libc::CPU_SET(self.0, &mut set);
			let mut set: dpdk_ffi::rte_cpuset_t = mem::transmute(set);
			dpdk_ffi::rte_thread_set_affinity(&mut set).to_result(DpdkError::from_errno)?;
		}
		CURRENT_CORE_ID.with(|tls| tls.set(*self));
		Ok(())
	}
}

impl fmt::Debug for CoreId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "core{}", self.0)
	}
}

thread_local! {
	static CURRENT_CORE_ID: Cell<CoreId> = Cell::new(CoreId::ANY);
}

/// Initializes the Environment Abstraction Layer (EAL)
pub(crate) fn eal_init(args: Vec<String>) -> Fallible<()> {
	debug!(arguments=?args);
	let len = args.len() as raw::c_int;
	let args = args.into_iter().map(|s| s.to_cstring()).collect::<Vec<_>>();
	let mut ptrs = args
		.iter()
		.map(|s| s.as_ptr() as *mut raw::c_char)
		.collect::<Vec<_>>();

	let res = unsafe { dpdk_ffi::rte_eal_init(len, ptrs.as_mut_ptr()) };
	debug!("EAL parsed {} arguments", res);

	res.to_result(DpdkError::from_errno).map(|_| ())
}

/// Cleans up the Environment Abstraction Layer (EAL).
pub(crate) fn eal_cleanup() -> Fallible<()> {
	unsafe {
		dpdk_ffi::rte_eal_cleanup()
			.to_result(DpdkError::from_errno)
			.map(|_| ())
	}
}

// Returns the `MacAddr` of a port.
fn eth_macaddr_get(port_id: u16) -> MacAddr {
	let mut addr = dpdk_ffi::rte_ether_addr::default();
	unsafe {
		dpdk_ffi::rte_eth_macaddr_get(port_id, &mut addr);
	}
	addr.addr_bytes.into()
}

/// Frees the `rte_mbuf` in bulk
pub(crate) fn mbuf_free_bulk(mbufs: Vec<*mut dpdk_ffi::rte_mbuf>) {
	assert!(!mbufs.is_empty());

	let mut to_free = Vec::with_capacity(mbufs.len());
	let pool = unsafe { (*mbufs[0]).pool };

	for mbuf in mbufs.into_iter() {
		if pool == unsafe { (*mbuf).pool } {
			to_free.push(mbuf as *mut raw::c_void);
		} else {
			unsafe {
				let len = to_free.len();
				dpdk_ffi::_rte_mempool_put_bulk(pool, to_free.as_ptr(), len as u32);
				to_free.set_len(0);
			}
			to_free.push(mbuf as *mut raw::c_void);
		}
	}

	unsafe {
		let len = to_free.len();
		dpdk_ffi::_rte_mempool_put_bulk(pool, to_free.as_ptr(), len as u32);
		to_free.set_len(0);
	}
}
