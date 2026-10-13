//! A pass-through body that copies what flows by, up to a cap.

use bytes::Bytes;
use hyper::body::{Body, Frame, SizeHint};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Bytes captured from one direction of a flow.
pub struct Capture {
    cap: usize,
    inner: Mutex<CaptureInner>,
}

#[derive(Default)]
struct CaptureInner {
    buf: Vec<u8>,
    size: u64,
    truncated: bool,
    ended_at: Option<Instant>,
}

pub struct Captured {
    pub bytes: Bytes,
    pub size: u64,
    pub truncated: bool,
    pub ended_at: Option<Instant>,
}

impl Capture {
    pub fn new(cap: usize) -> Arc<Capture> {
        Arc::new(Capture {
            cap,
            inner: Mutex::new(CaptureInner::default()),
        })
    }

    pub(crate) fn push(&self, data: &[u8]) {
        let mut g = self.inner.lock().unwrap();
        g.size += data.len() as u64;
        let room = self.cap.saturating_sub(g.buf.len());
        let take = room.min(data.len());
        g.buf.extend_from_slice(&data[..take]);
        if take < data.len() {
            g.truncated = true;
        }
    }

    fn mark_end(&self) {
        let mut g = self.inner.lock().unwrap();
        g.ended_at.get_or_insert_with(Instant::now);
    }

    /// Move the captured bytes out.
    pub fn take(&self) -> Captured {
        let mut g = self.inner.lock().unwrap();
        Captured {
            bytes: Bytes::from(std::mem::take(&mut g.buf)),
            size: g.size,
            truncated: g.truncated,
            ended_at: g.ended_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyEnd {
    Complete,
    /// The body stream failed.
    Error(String),
    /// The body was dropped before it finished: the peer went away.
    Aborted,
}

type OnEnd = Box<dyn FnOnce(BodyEnd) + Send + Sync + 'static>;

pub struct Tee<B: Body> {
    inner: B,
    capture: Arc<Capture>,
    on_end: Option<OnEnd>,
}

impl<B: Body> Tee<B> {
    pub fn new(
        inner: B,
        capture: Arc<Capture>,
        on_end: impl FnOnce(BodyEnd) + Send + Sync + 'static,
    ) -> Self {
        Tee {
            inner,
            capture,
            on_end: Some(Box::new(on_end)),
        }
    }

    fn finish(&mut self, end: BodyEnd) {
        if let Some(cb) = self.on_end.take() {
            if end == BodyEnd::Complete {
                self.capture.mark_end();
            }
            cb(end);
        }
    }
}

impl<B> Body for Tee<B>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: Into<BoxError>,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    this.capture.push(data);
                }
                if this.inner.is_end_stream() {
                    // The last frame carried data; do not wait for another poll.
                    this.finish(BodyEnd::Complete);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(e))) => {
                let e: BoxError = e.into();
                this.finish(BodyEnd::Error(e.to_string()));
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                this.finish(BodyEnd::Complete);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B: Body> Drop for Tee<B> {
    fn drop(&mut self) {
        // hyper never polls a body that is already at its end (a GET, a 204,
        // a HEAD response), so that case finishes here.
        let end = if self.inner.is_end_stream() {
            BodyEnd::Complete
        } else {
            BodyEnd::Aborted
        };
        self.finish(end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::{BodyExt, Empty, Full};
    use std::sync::atomic::{AtomicU8, Ordering};

    struct Never;
    impl Body for Never {
        type Data = Bytes;
        type Error = BoxError;
        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn captures_up_to_cap_and_counts_all() {
        let cap = Capture::new(4);
        let seen = Arc::new(AtomicU8::new(0));
        let s = seen.clone();
        let body = Full::new(Bytes::from_static(b"0123456789"));
        let tee = Tee::new(body, cap.clone(), move |end| {
            assert_eq!(end, BodyEnd::Complete);
            s.store(1, Ordering::SeqCst);
        });
        let all = tee.collect().await.unwrap().to_bytes();
        assert_eq!(all.len(), 10);
        let c = cap.take();
        assert_eq!(&c.bytes[..], b"0123");
        assert_eq!(c.size, 10);
        assert!(c.truncated);
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn drop_before_end_reports_aborted() {
        let got = Arc::new(Mutex::new(None));
        let g = got.clone();
        let tee = Tee::new(Never, Capture::new(16), move |end| {
            *g.lock().unwrap() = Some(end);
        });
        drop(tee);
        assert_eq!(got.lock().unwrap().clone(), Some(BodyEnd::Aborted));
    }

    #[test]
    fn empty_body_never_polled_still_completes() {
        let got = Arc::new(Mutex::new(None));
        let g = got.clone();
        let tee = Tee::new(Empty::<Bytes>::new(), Capture::new(16), move |end| {
            *g.lock().unwrap() = Some(end);
        });
        drop(tee);
        assert_eq!(got.lock().unwrap().clone(), Some(BodyEnd::Complete));
    }
}
