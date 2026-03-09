//! Compat shim for `IndexedVal::to_index()` and `IndexedVal::to_val()`.
//!
//! In nightlies < 2025-07-05 (commit-date < 2025-07-04), `IndexedVal` is a
//! public trait on `stable_mir::ty` providing `to_index(&self) -> usize` and
//! `to_val(usize) -> Self`. In later nightlies the trait became `pub(crate)`,
//! so external code can no longer call these methods.
//!
//! This module provides free functions `to_index` and `to_val` that work on
//! both sides of the breakpoint:
//!
//! - **Old nightlies**: delegate to the trait methods directly.
//! - **New nightlies**: extract the inner `usize` via a minimal serde
//!   `Serializer` (for `to_index`), or construct the newtype via `transmute`
//!   with a compile-time size assertion (for `to_val`).
//!
//! All affected types (`Ty`, `Span`, `AllocId`, `VariantIdx`, etc.) are
//! single-field newtypes around `usize` with `#[derive(Serialize)]`.

#[cfg(not(smir_no_indexed_val))]
use super::stable_mir;

// ---- Old nightlies: IndexedVal is public, just delegate ----

#[cfg(not(smir_no_indexed_val))]
pub use stable_mir::ty::IndexedVal;

#[cfg(not(smir_no_indexed_val))]
pub fn to_index<T: IndexedVal>(val: &T) -> usize {
    val.to_index()
}

#[cfg(not(smir_no_indexed_val))]
pub fn to_val<T: IndexedVal>(index: usize) -> T {
    T::to_val(index)
}

// ---- New nightlies: IndexedVal is pub(crate), use serde/transmute ----

#[cfg(smir_no_indexed_val)]
pub fn to_index<T: super::serde::Serialize>(val: &T) -> usize {
    super::serde::Serialize::serialize(val, UsizeExtractor)
        .expect("to_index: type did not serialize as a usize newtype")
}

#[cfg(smir_no_indexed_val)]
pub fn to_val<T>(index: usize) -> T {
    // These types are #[derive(Serialize, Copy, Clone)] newtypes around usize.
    // A single-field struct has the same layout as its field.
    assert!(
        std::mem::size_of::<T>() == std::mem::size_of::<usize>(),
        "to_val: type is not usize-sized"
    );
    unsafe { std::mem::transmute_copy(&index) }
}

// Minimal Serializer that extracts a usize from a newtype(usize) chain.
#[cfg(smir_no_indexed_val)]
struct UsizeExtractor;

#[cfg(smir_no_indexed_val)]
impl super::serde::Serializer for UsizeExtractor {
    type Ok = usize;
    type Error = UsizeExtractError;
    type SerializeSeq = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeTuple = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeTupleStruct = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeTupleVariant = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeMap = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeStruct = super::serde::ser::Impossible<usize, Self::Error>;
    type SerializeStructVariant = super::serde::ser::Impossible<usize, Self::Error>;

    fn serialize_newtype_struct<T: ?Sized + super::serde::Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<usize, Self::Error> {
        // Recurse: the inner value should serialize as u64/usize.
        value.serialize(UsizeExtractor)
    }

    fn serialize_u64(self, v: u64) -> Result<usize, Self::Error> {
        Ok(v as usize)
    }

    // All other methods are unsupported; the types we care about only hit
    // serialize_newtype_struct -> serialize_u64.
    fn serialize_bool(self, _: bool) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_i8(self, _: i8) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_i16(self, _: i16) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_i32(self, _: i32) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_i64(self, _: i64) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_u8(self, _: u8) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_u16(self, _: u16) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_u32(self, _: u32) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_f32(self, _: f32) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_f64(self, _: f64) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_char(self, _: char) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_str(self, _: &str) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_none(self) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_some<T: ?Sized + super::serde::Serialize>(self, _: &T) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_unit(self) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_newtype_variant<T: ?Sized + super::serde::Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<usize, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(UsizeExtractError)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(UsizeExtractError)
    }
}

#[cfg(smir_no_indexed_val)]
#[derive(Debug)]
struct UsizeExtractError;

#[cfg(smir_no_indexed_val)]
impl std::fmt::Display for UsizeExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "type did not serialize as a usize newtype")
    }
}

#[cfg(smir_no_indexed_val)]
impl std::error::Error for UsizeExtractError {}

#[cfg(smir_no_indexed_val)]
impl super::serde::ser::Error for UsizeExtractError {
    fn custom<T: std::fmt::Display>(_msg: T) -> Self {
        UsizeExtractError
    }
}
