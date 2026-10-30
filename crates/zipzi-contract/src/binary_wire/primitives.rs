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
