use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::Stream;

use crate::request_log::{parse_usage_from_bytes, PendingRequest, RequestLogStore, UsageSnapshot};

pub struct LoggingByteStream<S> {
    inner: S,
    pending: PendingRequest,
    log: RequestLogStore,
    usage: Option<UsageSnapshot>,
    finished: bool,
}

impl<S> LoggingByteStream<S> {
    pub fn new(inner: S, pending: PendingRequest, log: RequestLogStore) -> Self {
        Self {
            inner,
            pending,
            log,
            usage: None,
            finished: false,
        }
    }

    fn finalize(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let entry = self.log.finalize(self.pending.clone(), self.usage.clone());
        let log = self.log.clone();
        tokio::spawn(async move {
            log.push(entry).await;
        });
    }
}

impl<S> Drop for LoggingByteStream<S> {
    fn drop(&mut self) {
        self.finalize();
    }
}

impl<S, E> Stream for LoggingByteStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        let inner = Pin::new(&mut self.inner);
        match inner.poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                if let Some(usage) = parse_usage_from_bytes(&chunk) {
                    self.usage = Some(usage);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(err))) => {
                self.finalize();
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(None) => {
                self.finalize();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
