use super::super::error::BeaconError;
use super::EventStream;
use futures::StreamExt;
use relay_entity::BeaconEvent;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::Stream;
use tracing::{error, warn};

pub struct BeaconEventsStream {
    payload_attributes: EventStream,
    new_head: EventStream,
}

impl BeaconEventsStream {
    pub fn new(payload_attributes: EventStream, new_head: EventStream) -> Self {
        Self {
            payload_attributes,
            new_head,
        }
    }
}

impl Stream for BeaconEventsStream {
    type Item = Result<BeaconEvent, BeaconError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.payload_attributes.as_mut().poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(event))) => return Poll::Ready(Some(Ok(event))),
            Poll::Ready(Some(Err(err))) => {
                error!(target: "beacon_events", error = %err, "payload_attributes stream error");
                return Poll::Ready(None);
            }
            Poll::Ready(None) => {
                warn!(target: "beacon_events", "payload_attributes stream ended");
            }
            Poll::Pending => {}
        }

        match self.new_head.as_mut().poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(event))) => return Poll::Ready(Some(Ok(event))),
            Poll::Ready(Some(Err(err))) => {
                error!(target: "beacon_events", error = %err, "head stream error");
                return Poll::Ready(None);
            }
            Poll::Ready(None) => {
                error!(target: "beacon_events", "head stream ended");
                return Poll::Ready(None);
            }
            Poll::Pending => {}
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_entity::{HeadEvent, PayloadAttributesEvent};

    fn make_head(slot: u64) -> Result<BeaconEvent, BeaconError> {
        Ok(BeaconEvent::Head(HeadEvent {
            slot,
            block: "0xabc".into(),
            epoch_transition: false,
            execution_optimistic: false,
        }))
    }

    fn make_pa() -> Result<BeaconEvent, BeaconError> {
        Ok(BeaconEvent::PayloadAttributes(PayloadAttributesEvent {
            version: "deneb".into(),
            data: relay_entity::PayloadAttributesData {
                proposal_slot: 1,
                parent_block_hash: "0xdef".into(),
                payload_attributes: relay_entity::InnerPayloadAttributes {
                    timestamp: 1000,
                    prev_randao: "0x123".into(),
                    suggested_fee_recipient: "0xdead".into(),
                },
            },
        }))
    }

    #[tokio::test]
    async fn test_merged_stream_pa_first() {
        let pa = Box::pin(futures::stream::iter(vec![make_pa()]));
        let head = Box::pin(futures::stream::iter(vec![make_head(1)]));
        let mut stream = BeaconEventsStream::new(pa, head);

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, BeaconEvent::PayloadAttributes(_)));

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, BeaconEvent::Head(_)));
    }

    #[tokio::test]
    async fn test_merged_stream_pa_error_terminates() {
        let pa: EventStream = Box::pin(futures::stream::iter(vec![
            make_pa(),
            Err(BeaconError::Sse("test error".into())),
        ]));
        let head: EventStream = Box::pin(futures::stream::iter(vec![make_head(1)]));
        let mut stream = BeaconEventsStream::new(pa, head);

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, BeaconEvent::PayloadAttributes(_)));

        assert!(
            stream.next().await.is_none(),
            "stream should terminate on pa error"
        );
    }

    #[tokio::test]
    async fn test_merged_stream_head_error_terminates() {
        let pa: EventStream = Box::pin(futures::stream::iter(vec![make_pa()]));
        let head: EventStream = Box::pin(futures::stream::iter(vec![Err(BeaconError::Sse(
            "test error".into(),
        ))]));
        let mut stream = BeaconEventsStream::new(pa, head);

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, BeaconEvent::PayloadAttributes(_)));

        assert!(
            stream.next().await.is_none(),
            "stream should terminate on head error"
        );
    }
}
