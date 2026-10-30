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
