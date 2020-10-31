/*
 * Created on Wed Oct 28 2020:13:52:09
 * Created by Ratnadeep Bhattacharya
 */

use crate::warn;
use failure::{Fail, Fallible};
use std::{
	ffi::{CStr, CString},
	os::raw,
	ptr::NonNull,
};

/// Simplify `*const c_char` or [c_char] to `&str` conversion
pub(crate) trait AsStr {
	fn as_str(&self) -> &str;
}

impl AsStr for *const raw::c_char {
	#[inline]
	fn as_str(&self) -> &str {
		unsafe {
			CStr::from_ptr(*self).to_str().unwrap_or_else(|_| {
				warn!("invalid UTF8 data");
				Default::default()
			})
		}
	}
}

impl AsStr for [raw::c_char] {
	#[inline]
	fn as_str(&self) -> &str {
		unsafe {
			CStr::from_ptr(self.as_ptr()).to_str().unwrap_or_else(|_| {
				warn!("invalid UTF8 data");
				Default::default()
			})
		}
	}
}

/// Simplify `String` and `&str` to `CString` conversion.
pub(crate) trait ToCString {
	fn to_cstring(self) -> CString;
}

impl ToCString for String {
	#[inline]
	fn to_cstring(self) -> CString {
		CString::new(self).unwrap()
	}
}

impl ToCString for &str {
	#[inline]
	fn to_cstring(self) -> CString {
		CString::new(self).unwrap()
	}
}

/// Simplify dpdk FFI binding's return to a `Result` type.
///
/// # Example
///
/// ```
/// ffi::rte_eth_add_tx_callback(..., ..., ..., ...)
///     .to_result(|_| {
///         DpdkError::new()
/// })?;
/// ```
pub(crate) trait ToResult {
	type Ok;

	fn to_result<E: Fail, F>(self, f: F) -> Fallible<Self::Ok>
	where
		F: FnOnce(Self) -> E,
		Self: Sized;
}

impl<T> ToResult for *mut T {
	type Ok = NonNull<T>;

	#[inline]
	fn to_result<E: Fail, F>(self, f: F) -> Fallible<Self::Ok>
	where
		F: FnOnce(Self) -> E,
	{
		NonNull::new(self).ok_or_else(|| f(self).into())
	}
}

impl<T> ToResult for *const T {
	type Ok = *const T;

	#[inline]
	fn to_result<E: Fail, F>(self, f: F) -> Fallible<Self::Ok>
	where
		F: FnOnce(Self) -> E,
	{
		if self.is_null() {
			Err(f(self).into())
		} else {
			Ok(self)
		}
	}
}

impl ToResult for raw::c_int {
	type Ok = u32;

	#[inline]
	fn to_result<E: Fail, F>(self, f: F) -> Fallible<Self::Ok>
	where
		F: FnOnce(Self) -> E,
	{
		if self >= 0 {
			Ok(self as u32)
		} else {
			Err(f(self).into())
		}
	}
}
