/*
 * Created on Wed Oct 28 2020:22:24:13
 * Created by Ratnadeep Bhattacharya
 */

use super::{DpdkError, SocketId};
use crate::{
	debug,
	ffi::{AsStr, ToCString, ToResult},
	info,
};
use failure::{Fail, Fallible};
use std::{
	cell::Cell,
	collections::HashMap,
	fmt,
	os::raw,
	ptr::{self, NonNull},
	// sync::atomic::{AtomicUsize, Ordering},
};

/// A memory pool is an allocator of message buffers, or `Mbuf`
/// For best performance, each socket should have a dedicated Mempool
pub struct Mempool {
	raw: NonNull<dpdk_ffi::rte_mempool>,
}

impl Mempool {
	/// Creates a new `Mempool` for `Mbuf`.
	///
	/// `capacity` is the maximum number of `Mbuf` the `Mempool` can hold.
	/// The optimum size (in terms of memory usage) is when n is a power
	/// of two minus one.
	///
	/// `cache_size` is the per core object cache. If cache_size is non-zero,
	/// the library will try to limit the accesses to the common lockless
	/// pool. The cache can be disabled if the argument is set to 0.
	///
	/// `socket_id` is the socket where the memory should be allocated. The
	/// value can be `SocketId::ANY` if there is no constraint.
	///
	/// # Errors
	///
	/// If allocation fails, then `DpdkError` is returned.
	pub fn new(
		name: String,
		capacity: usize,
		cache_size: usize,
		socket_id: SocketId,
	) -> Fallible<Self> {
		let raw = unsafe {
			dpdk_ffi::rte_pktmbuf_pool_create(
				name.clone().to_cstring().as_ptr(),
				capacity as raw::c_uint,
				cache_size as raw::c_uint,
				0,
				dpdk_ffi::RTE_MBUF_DEFAULT_BUF_SIZE as u16,
				socket_id.raw(),
			)
			.to_result(|_| DpdkError::new())?
		};
		info!("created {}.", name);
		Ok(Self { raw })
	}

	/// Returns the raw struct required for FFI calls
	#[inline]
	pub fn raw(&self) -> &dpdk_ffi::rte_mempool {
		unsafe { self.raw.as_ref() }
	}

	/// Returns the raw struct required for FFI calls
	#[inline]
	pub fn raw_mut(&mut self) -> &mut dpdk_ffi::rte_mempool {
		unsafe { self.raw.as_mut() }
	}

	/// Returns the name of the mempool
	#[inline]
	pub fn name(&self) -> &str {
		self.raw().name[..].as_str()
	}
}

impl fmt::Debug for Mempool {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let raw = self.raw();
		f.debug_struct(self.name())
			.field("capacity", &raw.size)
			.field("cache size", &raw.cache_size)
			.field("flags", &format_args!("{:#x}", raw.flags))
			.field("socket", &raw.socket_id)
			.finish()
	}
}

impl Drop for Mempool {
	fn drop(&mut self) {
		debug!("freeing {}", self.name());

		unsafe {
			dpdk_ffi::rte_mempool_free(self.raw_mut());
		}
	}
}

thread_local! {
	/// `Mempool` on the same socket as the current core
	///
	/// It's set when the core is initialized
	/// New `Mbuf` is allocated on this `Mempool` when executed on this core
	pub static MEMPOOL: Cell<*mut dpdk_ffi::rte_mempool> = Cell::new(ptr::null_mut());
}

/// Error indicating the `Mempool` is found or is exhausted
#[derive(Debug, Fail)]
pub enum MempoolError {
	#[fail(display = "Cannot allocate a new mbuf from mempool")]
	Exhausted,

	#[fail(display = "Mempool for {:?} not found.", _0)]
	NotFound(SocketId),
}

/// A specialized hash map of `SocketId` to `&mut Mempool`.
#[derive(Debug)]
pub struct MempoolMap<'a> {
	inner: HashMap<SocketId, &'a mut Mempool>,
}

impl<'a> MempoolMap<'a> {
	/// Create a new map from a mutable slice
	pub fn new(mempools: &'a mut [Mempool]) -> Self {
		let map = mempools
			.iter_mut()
			.map(|pool| {
				let socket = SocketId(pool.raw().socket_id);
				(socket, pool)
			})
			.collect::<HashMap<_, _>>();

		Self { inner: map }
	}

	/// Returns a mutable reference to the raw mempool corresponding to the
	/// socket id.
	///
	/// # Errors
	///
	/// If the value is not found, `MempoolError::NotFound` is returned
	pub fn get_raw(&mut self, socket_id: SocketId) -> Fallible<&mut dpdk_ffi::rte_mempool> {
		self.inner
			.get_mut(&socket_id)
			.ok_or_else(|| MempoolError::NotFound(socket_id).into())
			.map(|pool| pool.raw_mut())
	}
}

impl<'a> Default for MempoolMap<'a> {
	fn default() -> MempoolMap<'a> {
		MempoolMap {
			inner: HashMap::new(),
		}
	}
}
