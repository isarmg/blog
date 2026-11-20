// This file is part of ICU4X. For terms of use, please see the file
// called LICENSE at the top level of the ICU4X source tree
// (online at: https://github.com/unicode-org/icu4x/blob/main/LICENSE ).

use zerovec::ule::VarULE;

#[cfg(feature = "alloc")]
use zerovec::{ZeroMap, ZeroMap2d, maps::ZeroMapKV};

/// A trait that associates a [`VarULE`] type with a data struct.
///
/// Some data structs can be represented compactly as a single [`VarULE`],
/// such as `str` or a packed pattern. This trait allows for data providers
/// to use optimizations for such types.
///
/// ❗ Not all data structs benefit from this optimization. It works best when the
/// data struct is multiplied across a large number of data marker attributes.
///
/// The [`data_struct!`](crate::data_struct) macro provides an implementation
/// for data structs without a specialized compact representation.
pub trait MaybeAsVarULE {
    /// The [`VarULE`] type for this data struct, or `[()]`
    /// if it cannot be represented as [`VarULE`].
    type EncodedStruct: ?Sized + VarULE;
}

/// Associates a data struct with a compact representation when one is supplied.
#[macro_export]
macro_rules! data_struct {
    (<$generic:ident: $bound:tt> $ty:path $(, $(#[$attr:meta])*)?) => {
        impl<$generic: $bound> $crate::ule::MaybeAsVarULE for $ty {
            type EncodedStruct = [()];
        }
    };
    ($ty:path $(, $(#[$attr:meta])*)?) => {
        impl $crate::ule::MaybeAsVarULE for $ty {
            type EncodedStruct = [()];
        }
    };
    ($ty:ty, varule: $varule:ty $(,)?) => {
        impl<'data> $crate::ule::MaybeAsVarULE for $ty {
            type EncodedStruct = $varule;
        }
    };
}

#[cfg(feature = "alloc")]
impl<'a, K0, V> MaybeAsVarULE for ZeroMap<'a, K0, V>
where
    K0: ZeroMapKV<'a>,
    V: ZeroMapKV<'a>,
    K0: ?Sized,
    V: ?Sized,
{
    type EncodedStruct = [()];
}

#[cfg(feature = "alloc")]
impl<'a, K0, K1, V> MaybeAsVarULE for ZeroMap2d<'a, K0, K1, V>
where
    K0: ZeroMapKV<'a>,
    K1: ZeroMapKV<'a>,
    V: ZeroMapKV<'a>,
    K0: ?Sized,
    K1: ?Sized,
    V: ?Sized,
{
    type EncodedStruct = [()];
}

impl<T, const N: usize> MaybeAsVarULE for [T; N] {
    type EncodedStruct = [()];
}

impl MaybeAsVarULE for u16 {
    type EncodedStruct = [()];
}

impl<'a, V: VarULE + ?Sized> MaybeAsVarULE for zerovec::VarZeroCow<'a, V> {
    type EncodedStruct = V;
}
