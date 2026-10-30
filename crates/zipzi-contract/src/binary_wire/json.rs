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
