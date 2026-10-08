//! C ABI shim that lets the C++ game call into the ported Rust crates.
//!
//! This crate intentionally exposes the smallest useful surface for the Phase 0
//! foundation. It exists to prove the C++/Rust boundary builds and links; the
//! real engine types are exposed in later phases.
//!
//! Every function here is `extern "C"` and uses only C-compatible types. The
//! opaque [`CataRng`] handle is heap-allocated and must be released with
//! [`cata_rng_free`].

use std::ffi::{c_char, c_int};

use cata_core::rng::{djb2_hash, Rng};

/// Opaque handle to a seeded generator. Created by [`cata_rng_new`].
pub struct CataRng {
    inner: Rng,
}

/// Create a new generator from `seed`.
///
/// # Safety
/// The returned pointer must be freed exactly once with [`cata_rng_free`].
#[no_mangle]
pub extern "C" fn cata_rng_new(seed: u32) -> *mut CataRng {
    Box::into_raw(Box::new(CataRng {
        inner: Rng::new(seed),
    }))
}

/// Free a generator previously returned by [`cata_rng_new`].
///
/// # Safety
/// `ptr` must be a pointer returned by [`cata_rng_new`] that has not already
/// been freed, or null.
#[no_mangle]
pub unsafe extern "C" fn cata_rng_free(ptr: *mut CataRng) {
    if !ptr.is_null() {
        // SAFETY: caller upholds the contract that `ptr` came from `cata_rng_new`.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// Return the next raw value from the generator, or `0` if `ptr` is null.
///
/// # Safety
/// `ptr` must be null or a valid handle from [`cata_rng_new`].
#[no_mangle]
pub unsafe extern "C" fn cata_rng_bits(ptr: *mut CataRng) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    // SAFETY: `ptr` is non-null and, per the contract, a live handle.
    unsafe { (*ptr).inner.bits() }
}

/// Return an inclusive integer in `[lo, hi]`, or `lo` if `ptr` is null.
///
/// # Safety
/// `ptr` must be null or a valid handle from [`cata_rng_new`].
#[no_mangle]
pub unsafe extern "C" fn cata_rng_range(ptr: *mut CataRng, lo: c_int, hi: c_int) -> c_int {
    if ptr.is_null() {
        return lo;
    }
    // SAFETY: `ptr` is non-null and, per the contract, a live handle.
    unsafe { (*ptr).inner.range(lo, hi) }
}

/// The `djb2` string hash, matching the C++ `djb2_hash(const unsigned char *)`.
///
/// `input` must be a valid NUL-terminated string. Returns `0` for a null pointer.
///
/// # Safety
/// `input` must be null or point to a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cata_djb2_hash(input: *const c_char) -> c_int {
    if input.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees a NUL-terminated string.
    let bytes = unsafe { std::ffi::CStr::from_ptr(input) }.to_bytes();
    djb2_hash(bytes)
}

#[cfg(test)]
mod tests {
    use super::{cata_djb2_hash, cata_rng_bits, cata_rng_free, cata_rng_new, cata_rng_range};

    #[test]
    fn rng_handle_roundtrip() {
        // SAFETY: all pointers come from `cata_rng_new` and are freed once.
        unsafe {
            let handle = cata_rng_new(1);
            // The first minstd_rand0 output for seed 1 is 16807.
            assert_eq!(cata_rng_bits(handle), 16807);
            let v = cata_rng_range(handle, 2, 5);
            assert!((2..=5).contains(&v));
            cata_rng_free(handle);
        }
    }

    #[test]
    fn null_handles_are_safe() {
        // SAFETY: null is explicitly allowed by each function's contract.
        unsafe {
            assert_eq!(cata_rng_bits(std::ptr::null_mut()), 0);
            assert_eq!(cata_rng_range(std::ptr::null_mut(), 7, 9), 7);
            cata_rng_free(std::ptr::null_mut());
            assert_eq!(cata_djb2_hash(std::ptr::null()), 0);
        }
    }
}
