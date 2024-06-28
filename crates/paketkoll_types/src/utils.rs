//! Internal helpers

/// Serializes `buffer` to a lowercase hex string.
#[cfg(feature = "serde")]
pub(crate) fn buffer_to_hex<T, S>(buffer: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: serde::Serializer,
{
    let buffer = buffer.as_ref();
    // We only use this for checksum, so small buffers. On the stack it goes:
    let mut buf = [0u8; 128];
    let s = faster_hex::hex_encode(buffer, &mut buf)
        .expect("This shouldn't fail on the data we use it for");
    serializer.serialize_str(s[0..buffer.len() * 2].as_ref())
}
