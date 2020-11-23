/*
 * Created on Wed Oct 28 2020:21:46:15
 * Created by Ratnadeep Bhattacharya
 */

use super::MEMPOOL;
use crate::{
	dpdk::{DpdkError, MempoolError},
	ensure,
	ffi::ToResult,
	trace,
};
use failure::{Fail, Fallible};
use std::{
	fmt, mem,
	os::raw,
	ptr::{self, NonNull},
	slice,
};

/// A trait for returning the size type in bytes
///
/// Size of structs are used for bound checks when reading and writing packets
///
/// # Derivable
///
/// The `SizeOf` trait can be used with `#[derive]` and defaults to `std::mem::size_of::<Self>()`
///
/// ```
/// #[derive(SizeOf)]
/// pub struct Ipv4Header {
///     ...
/// }
/// ```
pub trait SizeOf {
	/// Return the size of a type in bytes
	fn size_of() -> usize;
}

impl SizeOf for () {
	fn size_of() -> usize {
		std::mem::size_of::<()>()
	}
}

impl SizeOf for u8 {
	fn size_of() -> usize {
		std::mem::size_of::<u8>()
	}
}

impl SizeOf for [u8; 2] {
	fn size_of() -> usize {
		std::mem::size_of::<[u8; 2]>()
	}
}

impl SizeOf for [u8; 16] {
	fn size_of() -> usize {
		std::mem::size_of::<[u8; 16]>()
	}
}

impl SizeOf for ::std::net::Ipv6Addr {
	fn size_of() -> usize {
		std::mem::size_of::<std::net::Ipv6Addr>()
	}
}

/// Error indicating buffer access failures
#[derive(Debug, Fail)]
pub enum BufferError {
	/// The offset exceeds the buffer length
	#[fail(display = "Offset {} exceed the buffer length {}", _0, _1)]
	BadOffset(usize, usize),

	/// The buffer is not resized
	#[fail(display = "Buffer is not resized")]
	NotResized,

	/// The struct exceeds the remaining buffer length
	#[fail(
		display = "Struct size {} exceeds the remaining buffer length {}",
		_0, _1
	)]
	OutOfBuffer(usize, usize),
}

/// A DPDK message buffer that carries the network packet
///
/// # Remarks
///
/// Multi-segment Mbuf is not supported.
/// It's the application's responsibilty to ensure that the ethernet device's MTU is less than the default size
/// of a single Mbuf segment (`RTE_MBUF_DEFAULT_DATAROOM` = 2048)
pub struct Mbuf {
	inner: MbufInner,
}

/// Original or Clone tagged variant of DPDK message buffer
enum MbufInner {
	/// Original version of the message buffer
	/// Should be freed when goes out of scope
	/// Pointer returned to DPDK
	Original(NonNull<dpdk_ffi::rte_mbuf>),
	/// A clone version of the message buffer
	/// should not be freed when it goes out of scope
	Clone(NonNull<dpdk_ffi::rte_mbuf>),
}

impl MbufInner {
	fn ptr(&self) -> &NonNull<dpdk_ffi::rte_mbuf> {
		match self {
			MbufInner::Original(raw) => raw,
			MbufInner::Clone(raw) => raw,
		}
	}

	fn ptr_mut(&mut self) -> &mut NonNull<dpdk_ffi::rte_mbuf> {
		match self {
			MbufInner::Original(ref mut raw) => raw,
			MbufInner::Clone(ref mut raw) => raw,
		}
	}
}

impl Mbuf {
	/// Creates a new message buffer
	#[inline]
	pub fn new() -> Fallible<Self> {
		let mempool = MEMPOOL.with(|tls| tls.get());
		let raw = unsafe {
			dpdk_ffi::_rte_pktmbuf_alloc(mempool).to_result(|_| MempoolError::Exhausted)?
		};

		Ok(Mbuf {
			inner: MbufInner::Original(raw),
		})
	}

	/// Create a new message buffer from a byte array
	#[inline]
	pub fn from_bytes(data: &[u8]) -> Fallible<Self> {
		let mut mbuf = Mbuf::new()?;
		mbuf.extend(0, data.len())?;
		mbuf.write_data_slice(0, data)?;
		Ok(mbuf)
	}

	/// Creates a new `Mbuf` from a raw pointer
	#[inline]
	pub unsafe fn from_ptr(ptr: *mut dpdk_ffi::rte_mbuf) -> Self {
		Mbuf {
			inner: MbufInner::Original(NonNull::new_unchecked(ptr)),
		}
	}

	/// Returns the raw struct needed for FFI calls
	#[inline]
	pub fn raw(&self) -> &dpdk_ffi::rte_mbuf {
		unsafe { self.inner.ptr().as_ref() }
	}

	/// Returns the raw struct needed for FFI calls
	#[inline]
	pub fn raw_mut(&mut self) -> &mut dpdk_ffi::rte_mbuf {
		unsafe { self.inner.ptr_mut().as_mut() }
	}

	/// Return mutable reference to the C struct for FFI calls
	#[inline]
	pub fn get_ptr(&mut self) -> *mut dpdk_ffi::rte_mbuf {
		unsafe { self.inner.ptr_mut().as_mut() }
	}

	/// Returns amount of data stored in the buffer
	#[inline]
	pub fn data_len(&self) -> usize {
		self.raw().data_len as usize
	}

	/// Returns the raw pointer from the offset
	#[inline]
	pub unsafe fn data_address(&self, offset: usize) -> *mut u8 {
		let raw = self.raw();
		(raw.buf_addr as *mut u8).offset(raw.data_off as isize + offset as isize)
	}

	/// Returns the amount of bytes left in the buffer
	#[inline]
	fn tailroom(&self) -> usize {
		let raw = self.raw();
		(raw.buf_len - raw.data_off - raw.data_len) as usize
	}

	/// Extends the data buffer at offset `len`
	///
	/// If the offset is not the end of data
	/// data after offset is shifted down to make room
	#[inline]
	pub fn extend(&mut self, offset: usize, len: usize) -> Fallible<()> {
		ensure!(len > 0, BufferError::NotResized);
		ensure!(offset <= self.data_len(), BufferError::NotResized);
		ensure!(len < self.tailroom(), BufferError::NotResized);

		// shift down data to make room
		let to_copy = self.data_len() - offset;
		if to_copy > 0 {
			unsafe {
				let src = self.data_address(offset);
				let dst = self.data_address(offset + len);
				ptr::copy(src, dst, to_copy); // this is an expensive copy op
			}
		}

		// do some record keeping
		self.raw_mut().data_len += len as u16;
		self.raw_mut().pkt_len += len as u32;

		Ok(())
	}

	/// Shrinks the data buffer at offset by `len` bytes
	///
	/// The data at offset is shifted up
	#[inline]
	pub fn shrink(&mut self, offset: usize, len: usize) -> Fallible<()> {
		ensure!(len > 0, BufferError::NotResized);
		ensure!(offset + len <= self.data_len(), BufferError::NotResized);

		// shifts up data to fill the room
		let to_copy = self.data_len() - offset - len;
		if to_copy > 0 {
			unsafe {
				let src = self.data_address(offset + len);
				let dst = self.data_address(offset);
				ptr::copy(src, dst, to_copy); // expensive copy
			}
		}

		// do some record keeping
		self.raw_mut().data_len -= len as u16;
		self.raw_mut().pkt_len -= len as u32;

		Ok(())
	}

	/// Resizes the data buffer
	#[inline]
	pub fn resize(&mut self, offset: usize, len: isize) -> Fallible<()> {
		if len < 0 {
			self.shrink(offset, -len as usize)
		} else {
			self.extend(offset, len as usize)
		}
	}

	/// Truncates the data buffer to len
	#[inline]
	pub fn truncate(&mut self, to_len: usize) -> Fallible<()> {
		ensure!(to_len < self.data_len(), BufferError::NotResized);

		self.raw_mut().data_len = to_len as u16;
		self.raw_mut().pkt_len = to_len as u32;

		Ok(())
	}

	/// Reads the data at offset as `T` and returns it as a raw pointer.
	#[inline]
	pub fn read_data<T: SizeOf>(&self, offset: usize) -> Fallible<NonNull<T>> {
		ensure!(
			offset < self.data_len(),
			BufferError::BadOffset(offset, self.data_len())
		);
		ensure!(
			offset + T::size_of() <= self.data_len(),
			BufferError::OutOfBuffer(T::size_of(), self.data_len() - offset)
		);

		unsafe {
			let item = self.data_address(offset) as *mut T;
			Ok(NonNull::new_unchecked(item))
		}
	}
	/// Writes `T` to the data buffer at offset and returns the new copy
	/// as a raw pointer.
	///
	/// Before writing to the data buffer, should call `Mbuf::extend` first
	/// to make sure enough space is allocated for the write and data is not
	/// being overridden.
	#[inline]
	pub fn write_data<T: SizeOf>(&mut self, offset: usize, item: &T) -> Fallible<NonNull<T>> {
		ensure!(
			offset + T::size_of() <= self.data_len(),
			BufferError::OutOfBuffer(T::size_of(), self.data_len() - offset)
		);

		unsafe {
			let src = item as *const T;
			let dst = self.data_address(offset) as *mut T;
			ptr::copy_nonoverlapping(src, dst, 1);
		}

		self.read_data(offset)
	}

	/// Reads the data at offset as a slice of `T` and returns the slice as
	/// a raw pointer.
	#[inline]
	pub fn read_data_slice<T: SizeOf>(
		&self,
		offset: usize,
		count: usize,
	) -> Fallible<NonNull<[T]>> {
		ensure!(
			offset < self.data_len(),
			BufferError::BadOffset(offset, self.data_len())
		);
		ensure!(
			offset + T::size_of() * count <= self.data_len(),
			BufferError::OutOfBuffer(T::size_of() * count, self.data_len() - offset)
		);

		unsafe {
			let item0 = self.data_address(offset) as *mut T;
			let slice = slice::from_raw_parts_mut(item0, count) as *mut [T];
			Ok(NonNull::new_unchecked(slice))
		}
	}

	/// Writes a slice of `T` to the data buffer at offset and returns the
	/// new copy as a raw pointer.
	///
	/// Before writing to the data buffer, should call `Mbuf::extend` first
	/// to make sure enough space is allocated for the write and data is not
	/// being overridden.
	#[inline]
	pub fn write_data_slice<T: SizeOf>(
		&mut self,
		offset: usize,
		slice: &[T],
	) -> Fallible<NonNull<[T]>> {
		let count = slice.len();

		ensure!(
			offset + T::size_of() * count <= self.data_len(),
			BufferError::OutOfBuffer(T::size_of() * count, self.data_len() - offset)
		);

		unsafe {
			let src = slice.as_ptr();
			let dst = self.data_address(offset) as *mut T;
			ptr::copy_nonoverlapping(src, dst, count);
		}

		self.read_data_slice(offset, count)
	}

	/// Acquires the underlying raw struct pointer.
	///
	/// The `Mbuf` is consumed. It is the caller's the responsibility to
	/// free the raw pointer after use. Otherwise the buffer is leaked.
	#[inline]
	pub fn into_ptr(self) -> *mut dpdk_ffi::rte_mbuf {
		let ptr = self.inner.ptr().as_ptr();
		mem::forget(self);
		ptr
	}

	/// Allocates a Vec of `Mbuf`s of `len` size.
	pub fn alloc_bulk(len: usize) -> Fallible<Vec<Mbuf>> {
		let mut ptrs = Vec::with_capacity(len);
		let mempool = MEMPOOL.with(|tls| tls.get());

		let mbufs = unsafe {
			dpdk_ffi::_rte_pktmbuf_alloc_bulk(mempool, ptrs.as_mut_ptr(), len as raw::c_uint)
				.to_result(DpdkError::from_errno)?;

			ptrs.set_len(len);
			ptrs.into_iter()
				.map(|ptr| Mbuf::from_ptr(ptr))
				.collect::<Vec<_>>()
		};

		Ok(mbufs)
	}

	/// Frees the message buffers in bulk.
	pub fn free_bulk(mbufs: Vec<Mbuf>) {
		let ptrs = mbufs.into_iter().map(Mbuf::into_ptr).collect::<Vec<_>>();
		super::mbuf_free_bulk(ptrs);
	}
}

impl fmt::Debug for Mbuf {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let raw = self.raw();
		f.debug_struct(&format!("mbuf@{:p}", raw.buf_addr))
			.field("buf_len", &raw.buf_len)
			.field("pkt_len", &raw.pkt_len)
			.field("data_len", &raw.data_len)
			.field("data_off", &raw.data_off)
			.finish()
	}
}

impl Drop for Mbuf {
	fn drop(&mut self) {
		match self.inner {
			MbufInner::Original(_) => {
				trace!("freeing mbuf@{:p}.", self.raw().buf_addr);
				unsafe {
					dpdk_ffi::_rte_pktmbuf_free(self.raw_mut());
				}
			}
			MbufInner::Clone(_) => (),
		}
	}
}
