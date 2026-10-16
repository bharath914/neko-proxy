//! Length-prefixed protobuf frames: 4 bytes big-endian, then one `Envelope`.

use crate::wire::Envelope;
use prost::Message;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("connection closed")]
    Closed,
    #[error("frame of {0} bytes is larger than the limit of {1}")]
    TooLarge(usize, usize),
    #[error("empty frame")]
    Empty,
    #[error("malformed envelope: {0}")]
    Malformed(String),
    #[error("envelope has no message")]
    NoMessage,
    #[error("frame did not arrive in time")]
    Slow,
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Read one frame. `idle` bounds the wait for its first byte (None = wait
/// forever); `frame` bounds the time from the first byte to the last, which
/// stops a client that trickles bytes.
///
/// Not cancel-safe: call it from a dedicated reader task.
pub async fn read_envelope<R: AsyncRead + Unpin>(
    r: &mut R,
    max: usize,
    idle: Option<Duration>,
    frame: Duration,
) -> Result<Envelope, FrameError> {
    let mut first = [0u8; 1];
    let n = match idle {
        Some(d) => tokio::time::timeout(d, r.read(&mut first))
            .await
            .map_err(|_| FrameError::Slow)??,
        None => r.read(&mut first).await?,
    };
    if n == 0 {
        return Err(FrameError::Closed);
    }
    tokio::time::timeout(frame, async {
        let mut rest = [0u8; 3];
        r.read_exact(&mut rest).await.map_err(eof_is_closed)?;
        let len = u32::from_be_bytes([first[0], rest[0], rest[1], rest[2]]) as usize;
        if len == 0 {
            return Err(FrameError::Empty);
        }
        if len > max {
            return Err(FrameError::TooLarge(len, max));
        }
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf).await.map_err(eof_is_closed)?;
        decode(&buf)
    })
    .await
    .map_err(|_| FrameError::Slow)?
}

fn eof_is_closed(e: io::Error) -> FrameError {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        FrameError::Closed
    } else {
        FrameError::Io(e)
    }
}

/// Decode a frame body. Rejects envelopes with no message set, since every
/// valid frame carries one.
pub fn decode(buf: &[u8]) -> Result<Envelope, FrameError> {
    let env = Envelope::decode(buf).map_err(|e| FrameError::Malformed(e.to_string()))?;
    if env.msg.is_none() {
        return Err(FrameError::NoMessage);
    }
    Ok(env)
}

/// Encode one frame, length prefix included.
pub fn encode(env: &Envelope) -> Vec<u8> {
    let len = env.encoded_len();
    let mut out = Vec::with_capacity(4 + len);
    out.extend_from_slice(&(len as u32).to_be_bytes());
    env.encode(&mut out).expect("vec has room");
    out
}

pub async fn write_envelope<W: AsyncWrite + Unpin>(w: &mut W, env: &Envelope) -> io::Result<()> {
    w.write_all(&encode(env)).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{envelope::Msg, Ping};

    fn ping() -> Envelope {
        Envelope {
            msg: Some(Msg::Ping(Ping { time_ms: 42 })),
        }
    }

    #[tokio::test]
    async fn roundtrip() {
        let bytes = encode(&ping());
        let mut r = &bytes[..];
        let got = read_envelope(&mut r, 1024, None, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(got, ping());
    }

    #[tokio::test]
    async fn rejects_oversized_before_reading_the_body() {
        let mut bytes = (2_000_000u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0; 8]);
        let mut r = &bytes[..];
        let err = read_envelope(&mut r, 1 << 20, None, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(2_000_000, _)));
    }

    #[tokio::test]
    async fn rejects_empty_garbage_and_truncated() {
        let mut r: &[u8] = &[0, 0, 0, 0];
        assert!(matches!(
            read_envelope(&mut r, 1024, None, Duration::from_secs(1)).await,
            Err(FrameError::Empty)
        ));
        let mut r: &[u8] = &[0, 0, 0, 3, 0xff, 0xff, 0xff];
        assert!(matches!(
            read_envelope(&mut r, 1024, None, Duration::from_secs(1)).await,
            Err(FrameError::Malformed(_))
        ));
        // A valid protobuf message with no oneof set.
        let mut r: &[u8] = &[0, 0, 0, 1, 0x7a];
        assert!(read_envelope(&mut r, 1024, None, Duration::from_secs(1))
            .await
            .is_err());
        let mut r: &[u8] = &[0, 0, 0, 10, 1, 2];
        assert!(matches!(
            read_envelope(&mut r, 1024, None, Duration::from_secs(1)).await,
            Err(FrameError::Closed)
        ));
        let mut r: &[u8] = &[];
        assert!(matches!(
            read_envelope(&mut r, 1024, None, Duration::from_secs(1)).await,
            Err(FrameError::Closed)
        ));
    }

    #[tokio::test]
    async fn trickling_client_times_out() {
        let (mut a, mut b) = tokio::io::duplex(64);
        tokio::spawn(async move {
            b.write_all(&[0, 0]).await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let err = read_envelope(&mut a, 1024, None, Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::Slow));
    }
}
