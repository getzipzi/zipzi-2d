//! Bounded binary fields for explicitly selected compiled authoring records.
//! Readers make a non-allocating cost pass before constructing any owned data.
use std::{
    collections::{BTreeMap, HashMap},
    hash::Hash,
    io::Write,
    sync::Arc,
};
#[derive(Debug)]
pub struct WireError(pub &'static str);
impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for WireError {}
impl From<std::io::Error> for WireError {
    fn from(_: std::io::Error) -> Self {
        Self("Binary output failed")
    }
}
pub type Result<T> = std::result::Result<T, WireError>;
pub const MAX_ELEMENTS: usize = 1 << 20;
pub const MAX_BYTES: usize = 256 << 20;
pub const fn signature(text: &str) -> u128 {
    let mut value = 0x6c62272e07bb014262b821756295c58du128;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        value ^= bytes[i] as u128;
        value = value.wrapping_mul(0x1000000000000000000013b);
        i += 1;
    }
    value
}
pub const fn combine(left: u128, right: u128) -> u128 {
    left.wrapping_mul(0x1000000000000000000013b) ^ right.rotate_left(23)
}
pub trait WireWrite {
    const SIGNATURE: u128;
    const VARIANTS: &'static [&'static str] = &[];
    const FIELD_LAYOUT: u128 = Self::SIGNATURE;
    const VARIANT_FIELDS: &'static [&'static [&'static str]] = &[];
    fn encode<W: Write>(&self, out: &mut W) -> Result<()>;
    /// Conservative adapter expansion, including Python/JSON record nodes.
    fn expansion_bytes(&self) -> usize;
    fn encode_vector<W: Write>(values: &[Self], out: &mut W) -> Result<()>
    where
        Self: Sized,
    {
        for value in values {
            value.encode(out)?;
        }
        Ok(())
    }
    fn vector_expansion(values: &[Self]) -> usize
    where
        Self: Sized,
    {
        values
            .iter()
            .fold(64, |n, v| n.saturating_add(v.expansion_bytes()))
    }
}
pub trait WireRead: WireWrite + Sized {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize>;
    fn decode(input: &mut Reader<'_>) -> Result<Self>;
    fn vector_cost(input: &mut Reader<'_>, count: usize) -> Result<usize> {
        if count > MAX_ELEMENTS {
            return Err(WireError("Binary sequence exceeds item limit"));
        }
        let mut bytes = 64usize;
        for _ in 0..count {
            bytes = bytes.saturating_add(Self::input_cost(input)?);
        }
        Ok(bytes)
    }
    fn decode_vector(input: &mut Reader<'_>, count: usize) -> Result<Vec<Self>> {
        if count > MAX_ELEMENTS {
            return Err(WireError("Binary sequence exceeds item limit"));
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| WireError("Binary allocation refused"))?;
        for _ in 0..count {
            values.push(Self::decode(input)?);
        }
        Ok(values)
    }
}
pub struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() > MAX_BYTES {
            Err(WireError("Binary payload exceeds byte limit"))
        } else {
            Ok(Self { data, offset: 0 })
        }
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|n| *n <= self.data.len())
            .ok_or(WireError("Truncated binary field"))?;
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
    pub fn count(&mut self) -> Result<usize> {
        Ok(u32::decode(self)? as usize)
    }
    pub fn finish(self) -> Result<()> {
        if self.offset == self.data.len() {
            Ok(())
        } else {
            Err(WireError("Trailing binary fields"))
        }
    }
}
// Include codecs in this module so wire signatures keep their existing paths.
include!("binary_wire/primitives.rs");
include!("binary_wire/collections.rs");
include!("binary_wire/json.rs");
include!("binary_wire/output.rs");

#[cfg(test)]
mod tests;
