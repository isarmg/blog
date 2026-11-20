#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![cfg_attr(not(feature = "std"), no_std)]
/*
Unsafe functions no longer implicitly create unsafe blocks (RFC 2585).
While newer bindgen versions follow this convention, we support Rust 1.63 which warns about
this behavior. We can simply silence these warnings.
*/
#![allow(unused_unsafe)]

#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_pointer_width = "64"
)))]
compile_error!("blog-platform-linux-abi requires Linux amd64");

/// C primitive types for the Linux AMD64 ABI.
#[cfg(any(feature = "std", feature = "no_std"))]
pub mod ctypes {
    pub use core::ffi::{
        c_char, c_double, c_float, c_int, c_long, c_longlong, c_schar, c_short, c_uchar, c_uint,
        c_ulong, c_ulonglong, c_ushort, c_void,
    };
}

// Confirm that our type definitions above match the actual type definitions.
#[path = "../../../../../test/foundation/platform/linux-abi/unit/src/lib/assertions.rs"]
#[cfg(test)]
mod assertions;

// We don't enable `derive_eq` in bindgen because adding `PartialEq`/`Eq` to
// *all* structs noticeably increases compile times. But we can add a few
// manual impls where they're especially useful.
#[cfg(feature = "general")]
impl PartialEq for general::__kernel_timespec {
    fn eq(&self, other: &Self) -> bool {
        ({
            let Self { tv_sec, tv_nsec } = self;
            (tv_sec, tv_nsec)
        }) == ({
            let Self { tv_sec, tv_nsec } = other;
            (tv_sec, tv_nsec)
        })
    }
}
#[cfg(feature = "general")]
impl Eq for general::__kernel_timespec {}

#[cfg(feature = "general")]
pub mod select_macros {
    use crate::ctypes::c_int;
    use crate::general::__kernel_fd_set;
    use core::mem::size_of;

    pub unsafe fn FD_CLR(fd: c_int, set: *mut __kernel_fd_set) {
        let bytes = set as *mut u8;
        if fd >= 0 {
            *bytes.add((fd / 8) as usize) &= !(1 << (fd % 8));
        }
    }

    pub unsafe fn FD_SET(fd: c_int, set: *mut __kernel_fd_set) {
        let bytes = set as *mut u8;
        if fd >= 0 {
            *bytes.add((fd / 8) as usize) |= 1 << (fd % 8);
        }
    }

    pub unsafe fn FD_ISSET(fd: c_int, set: *const __kernel_fd_set) -> bool {
        let bytes = set as *const u8;
        if fd >= 0 {
            *bytes.add((fd / 8) as usize) & (1 << (fd % 8)) != 0
        } else {
            false
        }
    }

    pub unsafe fn FD_ZERO(set: *mut __kernel_fd_set) {
        let bytes = set as *mut u8;
        core::ptr::write_bytes(bytes, 0, size_of::<__kernel_fd_set>());
    }
}

#[cfg(feature = "general")]
pub mod signal_macros {
    pub const SIG_DFL: super::general::__kernel_sighandler_t = None;

    /// Rust doesn't currently permit us to use `transmute` to convert the
    /// `SIG_IGN` value into a function pointer in a `const` initializer, so
    /// we make it a function instead.
    ///
    #[inline]
    pub const fn sig_ign() -> super::general::__kernel_sighandler_t {
        // Safety: This creates an invalid pointer, but the pointer type
        // includes `unsafe`, which covers the safety of calling it.
        Some(unsafe {
            core::mem::transmute::<usize, unsafe extern "C" fn(crate::ctypes::c_int)>(1)
        })
    }
}

#[cfg(feature = "elf")]
pub mod elf;

// Linux amd64 bindings; other architectures are outside the product contract.
#[cfg(feature = "auxvec")]
#[path = "x86_64/auxvec.rs"]
pub mod auxvec;
#[cfg(feature = "errno")]
#[path = "x86_64/errno.rs"]
pub mod errno;
#[cfg(feature = "general")]
#[path = "x86_64/general.rs"]
pub mod general;
#[cfg(feature = "ioctl")]
#[path = "x86_64/ioctl.rs"]
pub mod ioctl;
#[cfg(feature = "prctl")]
#[path = "x86_64/prctl.rs"]
pub mod prctl;
#[cfg(feature = "system")]
#[path = "x86_64/system.rs"]
pub mod system;
