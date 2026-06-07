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
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                log.push(entry).await;
            });
        } else {
            log.push_sync(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use futures_util::{stream, StreamExt};

    use super::*;
    use crate::request_log::{PendingRequest, RequestLogStore};

    fn sample_pending(model: &str) -> PendingRequest {
        PendingRequest {
            provider_id: "deepseek".into(),
            provider_name: "DeepSeek".into(),
            model: model.into(),
            path: "/v1/chat/completions".into(),
            stream: true,
            started: Instant::now(),
            status: 200,
        }
    }

    #[tokio::test]
    async fn forwards_chunks_and_records_usage_on_stream_end() {
        let model = format!("logged-stream-ok-{}", uuid::Uuid::new_v4());
        let inner = stream::iter(vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b"data: {\"choices\":[]}\n\n")),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(
                br#"data: {"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}

"#,
            )),
        ]);
        let log = RequestLogStore::new();
        let mut logged = LoggingByteStream::new(inner, sample_pending(&model), log.clone());

        let mut out = Vec::new();
        while let Some(item) = logged.next().await {
            out.push(item.unwrap());
        }
        drop(logged);
        let entry = wait_for_entry_with_model(&log, &model).await;
        assert_eq!(entry.input_tokens, 3);
        assert_eq!(entry.output_tokens, 2);
        assert_eq!(entry.stream, true);
        assert_eq!(out.len(), 2);
    }

    #[tokio::test]
    async fn finalizes_once_on_stream_error() {
        let model = format!("logged-stream-err-{}", uuid::Uuid::new_v4());
        let inner = stream::iter(vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b"chunk")),
            Err(std::io::Error::other("upstream reset")),
        ]);
        let log = RequestLogStore::new();
        let mut logged = LoggingByteStream::new(inner, sample_pending(&model), log.clone());

        let first = logged.next().await.unwrap().unwrap();
        assert_eq!(first, Bytes::from_static(b"chunk"));
        let err = logged.next().await.unwrap().unwrap_err();
        assert_eq!(err.to_string(), "upstream reset");
        assert!(logged.next().await.is_none());
        drop(logged);
        let entry = wait_for_entry_with_model(&log, &model).await;
        assert_eq!(entry.status, 200);
        assert_eq!(entry.path, "/v1/chat/completions");
    }

    async fn wait_for_entry_with_model(
        log: &RequestLogStore,
        model: &str,
    ) -> crate::request_log::RequestLogEntry {
        let started = std::time::Instant::now();
        loop {
            if let Some(entry) = log
                .list()
                .await
                .into_iter()
                .find(|entry| entry.model == model)
            {
                return entry;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            if started.elapsed() > std::time::Duration::from_secs(1) {
                panic!("timed out waiting for request log entry with model {model}");
            }
        }
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
