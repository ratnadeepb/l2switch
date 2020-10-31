/*
 * Created on Thu Oct 29 2020:10:29:14
 * Created by Ratnadeep Bhattacharya
 */

use super::SocketId;
use crate::{
	debug,
	dpdk::DpdkError,
	ffi::{AsStr, ToCString, ToResult},
	info,
};
use dpdk_ffi;
use failure::Fallible;
use std::{fmt, os::raw, ptr::NonNull};

const NO_FLAGS: u8 = 0;

/// A ring is intended to communicate between two DPDK processes by sending/receiving `Mbuf`.
/// For best performance, each socket should have a dedicated `Mempool`.
pub struct Ring {
	raw: NonNull<dpdk_ffi::rte_ring>,
}

impl Ring {
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
	pub fn new(name: String, capacity: usize, socket_id: SocketId) -> Fallible<Self> {
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
		Ok(Self { raw })
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

	/// Returns the name of the `Mempool`.
	#[inline]
	pub fn name(&self) -> &str {
		self.raw().name[..].as_str()
	}
}

impl fmt::Debug for Ring {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let raw = self.raw();
		unsafe {
			f.debug_struct(self.name())
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
