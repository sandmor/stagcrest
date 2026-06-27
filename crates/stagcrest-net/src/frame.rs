use thiserror::Error;

pub const MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("frame exceeds max size ({MAX_FRAME_BYTES} bytes)")]
    TooLarge,
    #[error("truncated frame header")]
    TruncatedHeader,
    #[error("truncated frame payload")]
    TruncatedPayload,
    #[error("postcard decode error: {0}")]
    Decode(postcard::Error),
    #[error("postcard encode error: {0}")]
    Encode(postcard::Error),
}

pub fn encode_payload<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, FrameError> {
    postcard::to_allocvec(value).map_err(FrameError::Encode)
}

pub fn decode_payload<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, FrameError> {
    postcard::from_bytes(bytes).map_err(FrameError::Decode)
}

pub fn wrap_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    let len = payload.len() as u32;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn read_frame_header(header: &[u8; 4]) -> usize {
    u32::from_le_bytes(*header) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_wrap() {
        let payload = b"hello";
        let framed = wrap_frame(payload).unwrap();
        assert_eq!(&framed[..4], &(5u32.to_le_bytes()));
        assert_eq!(&framed[4..], payload);
    }
}
