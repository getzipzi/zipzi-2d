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
