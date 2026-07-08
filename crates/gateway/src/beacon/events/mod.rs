use super::error::BeaconError;
use futures::StreamExt;
use relay_entity::{BeaconEvent, HeadEvent, PayloadAttributesEvent};
use reqwest::Client;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{Duration, sleep};
use tokio_stream::Stream;
use tracing::{debug, error, info, warn};
use url::Url;

pub mod stream;

const RETRY_DELAY_SECS: u64 = 5;

pub type EventStream = Pin<Box<dyn Stream<Item = Result<BeaconEvent, BeaconError>> + Send>>;

pub trait BeaconConnection: Send + Sync {
    fn stream(&self) -> impl Future<Output = EventStream> + Send;
}

/// Low-level SSE stream that parses raw bytes from reqwest into BeaconEvent.
struct SseByteStream {
    stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, BeaconError>> + Send>>,
    buffer: Vec<u8>,
}

impl SseByteStream {
    fn new(stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, BeaconError>> + Send>>) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    fn parse_events(data: &[u8]) -> Vec<Result<BeaconEvent, BeaconError>> {
        let text = match std::str::from_utf8(data) {
            Ok(s) => s,
            Err(_) => return vec![Err(BeaconError::Sse("invalid utf-8 in SSE".into()))],
        };

        text.split("\n\n")
            .filter(|block| !block.is_empty())
            .filter_map(|block| {
                let mut event_type: Option<&str> = None;
                let mut event_data = String::new();

                for line in block.lines() {
                    if let Some(val) = line.strip_prefix("event: ") {
                        event_type = Some(val.trim());
                    } else if let Some(val) = line.strip_prefix("data: ") {
                        if !event_data.is_empty() {
                            event_data.push(' ');
                        }
                        event_data.push_str(val.trim());
                    }
                }

                let event_type = event_type?;
                if event_data.is_empty() {
                    return None;
                }

                Some(match event_type {
                    "head" => serde_json::from_str::<HeadEvent>(&event_data)
                        .map(BeaconEvent::Head)
                        .map_err(|e| BeaconError::Sse(e.to_string())),
                    "payload_attributes" => {
                        serde_json::from_str::<PayloadAttributesEvent>(&event_data)
                            .map(BeaconEvent::PayloadAttributes)
                            .map_err(|e| BeaconError::Sse(e.to_string()))
                    }
                    _ => {
                        debug!(target: "beacon_events", event = event_type, "unhandled event");
                        return None;
                    }
                })
            })
            .collect()
    }
}

impl Stream for SseByteStream {
    type Item = Result<BeaconEvent, BeaconError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while let Poll::Ready(chunk) = self.stream.as_mut().poll_next(cx) {
            match chunk {
                Some(Ok(data)) => {
                    self.buffer.extend_from_slice(&data);
                    if let Some(pos) = self.buffer.windows(4).position(|w| w == b"\n\n") {
                        let complete = self.buffer[..pos].to_vec();
                        self.buffer.drain(..pos + 4);
                        let events = Self::parse_events(&complete);
                        if let Some(Ok(e)) = events.into_iter().find(|r| r.is_ok()) {
                            return Poll::Ready(Some(Ok(e)));
                        }
                    }
                }
                Some(Err(e)) => {
                    error!(target: "beacon_events", "SSE stream error: {:?}", e);
                    return Poll::Ready(Some(Err(e)));
                }
                None => {
                    info!(target: "beacon_events", "SSE stream ended");
                    return Poll::Ready(None);
                }
            }
        }
        Poll::Pending
    }
}

#[derive(Debug, Clone)]
pub struct BeaconEventsClient {
    url: Url,
    client: Client,
}

impl BeaconEventsClient {
    pub fn new(url: Url) -> Self {
        info!(target: "beacon_client", %url, "creating beacon events client");
        Self {
            client: Client::new(),
            url,
        }
    }

    fn subscribe_once(
        &self,
        topics: &[&str],
    ) -> impl Future<Output = Result<SseByteStream, BeaconError>> + Send {
        let client = self.client.clone();
        let url = format!("{}eth/v1/events?topics={}", self.url, topics.join(","));
        async move {
            info!(target: "beacon_client", %url, "subscribing to SSE events");
            let response = client.get(&url).send().await.map_err(BeaconError::Http)?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(BeaconError::Api {
                    status: status.as_u16(),
                    body,
                });
            }
            let byte_stream = response
                .bytes_stream()
                .map(|chunk| chunk.map_err(BeaconError::Http));
            Ok(SseByteStream::new(Box::pin(byte_stream)))
        }
    }

    async fn subscribe_with_retry(&self, topics: &[&str]) -> SseByteStream {
        loop {
            match self.subscribe_once(topics).await {
                Ok(s) => return s,
                Err(err) => {
                    warn!(target: "beacon_client", error = %err, "SSE subscribe failed, retrying in {RETRY_DELAY_SECS}s");
                    sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                }
            }
        }
    }

    async fn new_head(&self) -> EventStream {
        Box::pin(self.subscribe_with_retry(&["head"]).await)
    }

    async fn payload_attributes(&self) -> EventStream {
        Box::pin(self.subscribe_with_retry(&["payload_attributes"]).await)
    }
}

impl BeaconConnection for BeaconEventsClient {
    async fn stream(&self) -> EventStream {
        let pa = self.payload_attributes().await;
        let head = self.new_head().await;
        Box::pin(stream::BeaconEventsStream::new(pa, head))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_head_event() {
        let data = b"event: head\ndata: {\"slot\":\"123\",\"block\":\"0xabc\",\"epoch_transition\":false,\"execution_optimistic\":false}\n\n";
        let events = SseByteStream::parse_events(data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(BeaconEvent::Head(head)) => {
                assert_eq!(head.slot, 123);
                assert_eq!(head.block, "0xabc");
            }
            _ => panic!("expected Head event"),
        }
    }

    #[test]
    fn test_parse_payload_attributes_event() {
        let data = b"event: payload_attributes\ndata: {\"version\":\"deneb\",\"data\":{\"proposal_slot\":\"456\",\"parent_block_hash\":\"0xdef\",\"payload_attributes\":{\"timestamp\":\"1000\",\"prev_randao\":\"0x123\",\"suggested_fee_recipient\":\"0xdead\"}}}\n\n";
        let events = SseByteStream::parse_events(data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(BeaconEvent::PayloadAttributes(pa)) => {
                assert_eq!(pa.data.payload_attributes.timestamp, 1000);
            }
            _ => panic!("expected PayloadAttributes event"),
        }
    }

    #[test]
    fn test_parse_multiple_events() {
        let data = b"event: head\ndata: {\"slot\":\"1\",\"block\":\"0xa\",\"epoch_transition\":false,\"execution_optimistic\":false}\n\nevent: payload_attributes\ndata: {\"version\":\"deneb\",\"data\":{\"proposal_slot\":\"2\",\"parent_block_hash\":\"0xb\",\"payload_attributes\":{\"timestamp\":\"3\",\"prev_randao\":\"0xc\",\"suggested_fee_recipient\":\"0xd\"}}}\n\n";
        let events = SseByteStream::parse_events(data);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_skip_unknown_event() {
        let data = b"event: unknown\ndata: {\"foo\":\"bar\"}\n\nevent: head\ndata: {\"slot\":\"7\",\"block\":\"0x7\",\"epoch_transition\":false,\"execution_optimistic\":false}\n\n";
        let events = SseByteStream::parse_events(data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(BeaconEvent::Head(h)) => assert_eq!(h.slot, 7),
            _ => panic!("expected Head event"),
        }
    }
}
