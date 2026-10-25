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
macro_rules! number {
    ($ty:ty,$name:literal) => {
        impl WireWrite for $ty {
            const SIGNATURE: u128 = signature($name);
            fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
                out.write_all(&self.to_le_bytes())?;
                Ok(())
            }
            fn expansion_bytes(&self) -> usize {
                64
            }
        }
        impl WireRead for $ty {
            fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
                let _ = Self::decode(input)?;
                Ok(std::mem::size_of::<Self>())
            }
            fn decode(input: &mut Reader<'_>) -> Result<Self> {
                Ok(Self::from_le_bytes(
                    input.take(std::mem::size_of::<Self>())?.try_into().unwrap(),
                ))
            }
        }
    };
}
number!(u16, "u16-le");
number!(u32, "u32-le");
number!(u64, "u64-le");
number!(u128, "u128-le");
number!(i32, "i32-le");
number!(i64, "i64-le");
impl WireWrite for u8 {
    const SIGNATURE: u128 = signature("u8");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        out.write_all(&[*self])?;
        Ok(())
    }
    fn expansion_bytes(&self) -> usize {
        64
    }
    fn encode_vector<W: Write>(values: &[Self], out: &mut W) -> Result<()> {
        out.write_all(values)?;
        Ok(())
    }
    fn vector_expansion(values: &[Self]) -> usize {
        values.len().saturating_mul(2).saturating_add(128)
    }
}
impl WireRead for u8 {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        input.take(1)?;
        Ok(1)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        Ok(input.take(1)?[0])
    }
    fn vector_cost(input: &mut Reader<'_>, n: usize) -> Result<usize> {
        input.take(n)?;
        Ok(n.saturating_add(64))
    }
    fn decode_vector(input: &mut Reader<'_>, n: usize) -> Result<Vec<Self>> {
        let bytes = input.take(n)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n)
            .map_err(|_| WireError("Binary byte allocation refused"))?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
}
impl WireWrite for usize {
    const SIGNATURE: u128 = signature("usize-u64-le");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        (*self as u64).encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        64
    }
}
impl WireRead for usize {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        let _ = Self::decode(input)?;
        Ok(8)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        u64::decode(input)?
            .try_into()
            .map_err(|_| WireError("Integer exceeds host range"))
    }
}
macro_rules! float {
    ($ty:ty,$name:literal) => {
        impl WireWrite for $ty {
            const SIGNATURE: u128 = signature($name);
            fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
                if !self.is_finite() {
                    return Err(WireError("Non-finite binary float"));
                }
                out.write_all(&self.to_le_bytes())?;
                Ok(())
            }
            fn expansion_bytes(&self) -> usize {
                64
            }
        }
        impl WireRead for $ty {
            fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
                let _ = Self::decode(input)?;
                Ok(std::mem::size_of::<Self>())
            }
            fn decode(input: &mut Reader<'_>) -> Result<Self> {
                let value = Self::from_le_bytes(
                    input.take(std::mem::size_of::<Self>())?.try_into().unwrap(),
                );
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err(WireError("Non-finite binary float"))
                }
            }
        }
    };
}
float!(f32, "f32-le-finite");
float!(f64, "f64-le-finite");
impl WireWrite for bool {
    const SIGNATURE: u128 = signature("bool-u8");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        u8::from(*self).encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        16
    }
}
impl WireRead for bool {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        let _ = Self::decode(input)?;
        Ok(1)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        match u8::decode(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError("Invalid binary bool")),
        }
    }
}
impl WireWrite for String {
    const SIGNATURE: u128 = signature("utf8-u32");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        write_bytes(self.as_bytes(), out)
    }
    fn expansion_bytes(&self) -> usize {
        self.len().saturating_mul(4).saturating_add(128)
    }
}
impl WireWrite for &str {
    const SIGNATURE: u128 = String::SIGNATURE;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        write_bytes(self.as_bytes(), out)
    }
    fn expansion_bytes(&self) -> usize {
        self.len().saturating_mul(4).saturating_add(128)
    }
}
impl WireRead for String {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        let n = input.count()?;
        let bytes = input.take(n)?;
        std::str::from_utf8(bytes).map_err(|_| WireError("Invalid binary UTF-8"))?;
        Ok(n.saturating_add(32))
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        let n = input.count()?;
        Ok(std::str::from_utf8(input.take(n)?)
            .map_err(|_| WireError("Invalid binary UTF-8"))?
            .to_owned())
    }
}
fn write_bytes<W: Write>(bytes: &[u8], out: &mut W) -> Result<()> {
    let n: u32 = bytes
        .len()
        .try_into()
        .map_err(|_| WireError("Binary length exceeds u32"))?;
    n.encode(out)?;
    out.write_all(bytes)?;
    Ok(())
}
impl<T: WireWrite> WireWrite for Vec<T> {
    const SIGNATURE: u128 = combine(signature("vec-u32"), T::SIGNATURE);
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        let n: u32 = self
            .len()
            .try_into()
            .map_err(|_| WireError("Binary sequence too large"))?;
        n.encode(out)?;
        T::encode_vector(self, out)
    }
    fn expansion_bytes(&self) -> usize {
        T::vector_expansion(self)
    }
}
impl<T: WireRead> WireRead for Vec<T> {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        let n = input.count()?;
        T::vector_cost(input, n)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        let n = input.count()?;
        T::decode_vector(input, n)
    }
}
impl<T: WireWrite> WireWrite for Option<T> {
    const SIGNATURE: u128 = combine(signature("option-u8"), T::SIGNATURE);
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        match self {
            None => 0u8.encode(out),
            Some(v) => {
                1u8.encode(out)?;
                v.encode(out)
            }
        }
    }
    fn expansion_bytes(&self) -> usize {
        self.as_ref().map_or(16, WireWrite::expansion_bytes)
    }
}
impl<T: WireRead> WireRead for Option<T> {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        match u8::decode(input)? {
            0 => Ok(std::mem::size_of::<Self>()),
            1 => Ok(T::input_cost(input)?.saturating_add(16)),
            _ => Err(WireError("Invalid binary option")),
        }
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        match u8::decode(input)? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(input)?)),
            _ => Err(WireError("Invalid binary option")),
        }
    }
}
impl<T: WireWrite, const N: usize> WireWrite for [T; N] {
    const SIGNATURE: u128 = combine(combine(signature("fixed-array"), N as u128), T::SIGNATURE);
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        T::encode_vector(self, out)
    }
    fn expansion_bytes(&self) -> usize {
        T::vector_expansion(self)
    }
}
impl<T: WireRead, const N: usize> WireRead for [T; N] {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        T::vector_cost(input, N)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        T::decode_vector(input, N)?
            .try_into()
            .map_err(|_| WireError("Invalid binary array"))
    }
}
// Owning indirection changes storage, never the finite binary layout.
impl<T: WireWrite> WireWrite for Box<T> {
    const SIGNATURE: u128 = T::SIGNATURE;
    const FIELD_LAYOUT: u128 = T::FIELD_LAYOUT;
    const VARIANTS: &'static [&'static str] = T::VARIANTS;
    const VARIANT_FIELDS: &'static [&'static [&'static str]] = T::VARIANT_FIELDS;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        self.as_ref().encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        self.as_ref()
            .expansion_bytes()
            .saturating_add(std::mem::size_of::<Self>())
    }
}
impl<T: WireRead> WireRead for Box<T> {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        Ok(T::input_cost(input)?.saturating_add(std::mem::size_of::<Self>()))
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        Ok(Box::new(T::decode(input)?))
    }
}
impl<T: WireWrite> WireWrite for Arc<T> {
    const SIGNATURE: u128 = T::SIGNATURE;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        self.as_ref().encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        self.as_ref().expansion_bytes()
    }
}
impl<T: WireRead> WireRead for Arc<T> {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        Ok(T::input_cost(input)?.saturating_add(64))
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        Ok(Arc::new(T::decode(input)?))
    }
}
macro_rules! map {
    ($name:ident,$($bound:path),+)=>{
        impl<K:WireWrite $(+$bound)+,V:WireWrite> WireWrite for $name<K,V> {
            const SIGNATURE:u128=combine(combine(signature(stringify!($name)),K::SIGNATURE),V::SIGNATURE);
            fn encode<W:Write>(&self,out:&mut W)->Result<()> {let n:u32=self.len().try_into().map_err(|_|WireError("Binary map too large"))?;n.encode(out)?;for(k,v) in self{k.encode(out)?;v.encode(out)?;}Ok(())}
            fn expansion_bytes(&self)->usize {self.iter().fold(128,|n,(k,v)|n.saturating_add(k.expansion_bytes()).saturating_add(v.expansion_bytes()).saturating_add(128))}
        }
        impl<K:WireRead $(+$bound)+,V:WireRead> WireRead for $name<K,V> {
            fn input_cost(input:&mut Reader<'_>)->Result<usize> {let n=input.count()?;if n>MAX_ELEMENTS{return Err(WireError("Binary map too large"));}let mut cost=128usize;for _ in 0..n{cost=cost.saturating_add(K::input_cost(input)?).saturating_add(V::input_cost(input)?).saturating_add(128);}Ok(cost)}
            fn decode(input:&mut Reader<'_>)->Result<Self> {let n=input.count()?;if n>MAX_ELEMENTS{return Err(WireError("Binary map too large"));}let mut result=Self::new();for _ in 0..n {let k=K::decode(input)?;let v=V::decode(input)?;if result.insert(k,v).is_some(){return Err(WireError("Duplicate binary map key"));}}Ok(result)}
        }
    };
}
map!(BTreeMap, Ord);
map!(HashMap, Eq, Hash);
impl WireWrite for std::path::PathBuf {
    const SIGNATURE: u128 = signature("path-utf8");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        self.to_str()
            .ok_or(WireError("Non-UTF-8 binary path"))?
            .encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        self.as_os_str().len().saturating_mul(4).saturating_add(128)
    }
}
impl WireRead for std::path::PathBuf {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        String::input_cost(input)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        Ok(String::decode(input)?.into())
    }
}
impl<T: WireWrite> WireWrite for Arc<[T]> {
    const SIGNATURE: u128 = Vec::<T>::SIGNATURE;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        let n: u32 = self
            .len()
            .try_into()
            .map_err(|_| WireError("Binary sequence too large"))?;
        n.encode(out)?;
        T::encode_vector(self, out)
    }
    fn expansion_bytes(&self) -> usize {
        T::vector_expansion(self)
    }
}
impl WireRead for Arc<[u8]> {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        let n = input.count()?;
        input.take(n)?;
        Ok(n.saturating_add(64))
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        let n = input.count()?;
        Ok(input.take(n)?.into())
    }
}
impl WireWrite for serde_json::Value {
    const SIGNATURE: u128 = signature("extension-json-v1");
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        use serde_json::Value as V;
        match self {
            V::Null => 0u8.encode(out),
            V::Bool(v) => {
                1u8.encode(out)?;
                v.encode(out)
            }
            V::Number(n) => {
                if let Some(v) = n.as_i64() {
                    2u8.encode(out)?;
                    v.encode(out)
                } else if let Some(v) = n.as_u64() {
                    3u8.encode(out)?;
                    v.encode(out)
                } else {
                    4u8.encode(out)?;
                    n.as_f64().unwrap().encode(out)
                }
            }
            V::String(v) => {
                5u8.encode(out)?;
                v.encode(out)
            }
            V::Array(v) => {
                6u8.encode(out)?;
                v.encode(out)
            }
            V::Object(values) => {
                7u8.encode(out)?;
                let n: u32 = values
                    .len()
                    .try_into()
                    .map_err(|_| WireError("Extension map too large"))?;
                n.encode(out)?;
                for (k, v) in values {
                    k.encode(out)?;
                    v.encode(out)?;
                }
                Ok(())
            }
        }
    }
    fn expansion_bytes(&self) -> usize {
        match self {
            Self::Null | Self::Bool(_) | Self::Number(_) => 64,
            Self::String(s) => s.expansion_bytes(),
            Self::Array(v) => v.expansion_bytes(),
            Self::Object(v) => v.iter().fold(128, |n, (k, v)| {
                n.saturating_add(k.expansion_bytes())
                    .saturating_add(v.expansion_bytes())
                    .saturating_add(128)
            }),
        }
    }
}
impl WireRead for serde_json::Value {
    fn input_cost(input: &mut Reader<'_>) -> Result<usize> {
        json_cost(input, 0)
    }
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        json_decode(input, 0)
    }
}
fn json_cost(input: &mut Reader<'_>, depth: usize) -> Result<usize> {
    if depth > 128 {
        return Err(WireError("Extension nesting exceeds limit"));
    }
    let bytes = match u8::decode(input)? {
        0 => 0,
        1 => bool::input_cost(input)?,
        2 => i64::input_cost(input)?,
        3 => u64::input_cost(input)?,
        4 => f64::input_cost(input)?,
        5 => String::input_cost(input)?,
        tag @ (6 | 7) => {
            let n = input.count()?;
            if n > MAX_ELEMENTS {
                return Err(WireError("Extension entries exceed limit"));
            }
            let mut size = 64usize;
            for _ in 0..n {
                if tag == 7 {
                    size = size
                        .saturating_add(String::input_cost(input)?)
                        .saturating_add(128);
                }
                size = size.saturating_add(json_cost(input, depth + 1)?);
            }
            size
        }
        _ => return Err(WireError("Invalid binary extension tag")),
    };
    Ok(bytes.saturating_add(32))
}
fn json_decode(input: &mut Reader<'_>, depth: usize) -> Result<serde_json::Value> {
    use serde_json::Value as V;
    if depth > 128 {
        return Err(WireError("Extension nesting exceeds limit"));
    }
    Ok(match u8::decode(input)? {
        0 => V::Null,
        1 => V::Bool(bool::decode(input)?),
        2 => V::from(i64::decode(input)?),
        3 => V::from(u64::decode(input)?),
        4 => V::from(f64::decode(input)?),
        5 => V::String(String::decode(input)?),
        tag @ (6 | 7) => {
            let n = input.count()?;
            if n > MAX_ELEMENTS {
                return Err(WireError("Extension entries exceed limit"));
            }
            if tag == 6 {
                let mut values = Vec::new();
                values
                    .try_reserve_exact(n)
                    .map_err(|_| WireError("Extension allocation refused"))?;
                for _ in 0..n {
                    values.push(json_decode(input, depth + 1)?);
                }
                V::Array(values)
            } else {
                let mut values = serde_json::Map::new();
                for _ in 0..n {
                    let key = String::decode(input)?;
                    let value = json_decode(input, depth + 1)?;
                    if values.insert(key, value).is_some() {
                        return Err(WireError("Duplicate extension key"));
                    }
                }
                V::Object(values)
            }
        }
        _ => return Err(WireError("Invalid binary extension tag")),
    })
}
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| std::io::Error::other("Binary byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn encoded_len<T: WireWrite>(value: &T) -> Result<usize> {
    let mut counter = Counter(0);
    value.encode(&mut counter)?;
    Ok(counter.0)
}
pub fn encode<T: WireWrite>(value: &T) -> Result<Vec<u8>> {
    let length = encoded_len(value)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| WireError("Binary output allocation refused"))?;
    value.encode(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, zipzi_api_derive::BinaryWire)]
    struct Packet {
        name: String,
        points: Vec<[f32; 2]>,
        labels: BTreeMap<usize, Option<String>>,
        extension: serde_json::Value,
        #[serde(skip)]
        imported: Vec<f32>,
    }
    #[derive(Debug, PartialEq, zipzi_api_derive::BinaryWire)]
    enum Tagged {
        Unit,
        Packet(Packet),
        Fields { bytes: Vec<u8>, enabled: bool },
    }
    #[test]
    fn owning_indirection_preserves_layout_and_admits_its_allocation() {
        let value = vec![1u32, 7, 99];
        let bytes = encode(&value).unwrap();
        assert_eq!(Box::<Vec<u32>>::SIGNATURE, Vec::<u32>::SIGNATURE);
        assert_eq!(encode(&Box::new(value.clone())).unwrap(), bytes);
        let mut plain = Reader::new(&bytes).unwrap();
        let cost = Vec::<u32>::input_cost(&mut plain).unwrap();
        let mut measured = Reader::new(&bytes).unwrap();
        assert!(Box::<Vec<u32>>::input_cost(&mut measured).unwrap() > cost);
        measured.finish().unwrap();
        let mut input = Reader::new(&bytes).unwrap();
        assert_eq!(*Box::<Vec<u32>>::decode(&mut input).unwrap(), value);
        input.finish().unwrap();
    }
    #[test]
    fn concrete_records_roundtrip_without_json_and_skip_import_only_fields() {
        let value = Tagged::Packet(Packet {
            name: "表情 🦊".into(),
            points: vec![[1.25, -2.5], [3., 4.]],
            labels: BTreeMap::from([(7, Some("seven".into())), (19, None)]),
            extension: serde_json::json!({"a":[true,null,-3,1.5],"text":"x"}),
            imported: vec![99.],
        });
        let bytes = encode(&value).unwrap();
        assert_eq!(bytes.len(), encoded_len(&value).unwrap());
        let mut measured = Reader::new(&bytes).unwrap();
        let cost = Tagged::input_cost(&mut measured).unwrap();
        measured.finish().unwrap();
        assert!(cost >= bytes.len());
        let mut input = Reader::new(&bytes).unwrap();
        let decoded = Tagged::decode(&mut input).unwrap();
        input.finish().unwrap();
        let Tagged::Packet(mut expected) = value else {
            panic!()
        };
        expected.imported.clear();
        assert_eq!(decoded, Tagged::Packet(expected));
        assert_eq!(Tagged::VARIANTS, &["Unit", "Packet", "Fields"]);
    }
    #[test]
    fn byte_vectors_use_bulk_lengths_beyond_the_structural_item_limit() {
        let value = Tagged::Fields {
            bytes: vec![83; MAX_ELEMENTS + 17],
            enabled: true,
        };
        let bytes = encode(&value).unwrap();
        let mut input = Reader::new(&bytes).unwrap();
        Tagged::input_cost(&mut input).unwrap();
        input.finish().unwrap();
        let mut input = Reader::new(&bytes).unwrap();
        let Tagged::Fields {
            bytes: decoded,
            enabled,
        } = Tagged::decode(&mut input).unwrap()
        else {
            panic!()
        };
        assert!(enabled);
        assert_eq!(decoded.len(), MAX_ELEMENTS + 17);
        assert_eq!(decoded.capacity(), decoded.len());
    }
    #[test]
    fn malformed_lengths_floats_utf8_and_tags_are_rejected_in_the_cost_pass() {
        for payload in [
            vec![255, 255, 255, 255],
            vec![2, 0, 0, 0, 255],
            vec![1, 0, 0, 0, 0xff],
        ] {
            let mut input = Reader::new(&payload).unwrap();
            assert!(String::input_cost(&mut input).is_err());
        }
        let mut input = Reader::new(&[255, 255, 255, 255]).unwrap();
        assert!(Vec::<f32>::input_cost(&mut input).is_err());
        let nan = f32::NAN.to_le_bytes();
        assert!(f32::input_cost(&mut Reader::new(&nan).unwrap()).is_err());
        assert!(bool::input_cost(&mut Reader::new(&[2]).unwrap()).is_err());
        assert!(Option::<u32>::input_cost(&mut Reader::new(&[3]).unwrap()).is_err());
        assert!(Tagged::input_cost(&mut Reader::new(&99u32.to_le_bytes()).unwrap()).is_err());
        assert!(Reader::new(&[1]).unwrap().finish().is_err());
    }
}

impl<T: WireWrite> WireWrite for &T {
    const SIGNATURE: u128 = T::SIGNATURE;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        (*self).encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        (*self).expansion_bytes()
    }
}
impl<T: WireWrite> WireWrite for &[T] {
    const SIGNATURE: u128 = Vec::<T>::SIGNATURE;
    fn encode<W: Write>(&self, out: &mut W) -> Result<()> {
        let n: u32 = self
            .len()
            .try_into()
            .map_err(|_| WireError("Binary slice too large"))?;
        n.encode(out)?;
        T::encode_vector(self, out)
    }
    fn expansion_bytes(&self) -> usize {
        T::vector_expansion(self)
    }
}
