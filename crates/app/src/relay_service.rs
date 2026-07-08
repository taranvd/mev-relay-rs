use futures::StreamExt;
use relay_datastore::Storage;
use relay_entity::{BeaconEvent, HeadSlot, PayloadAttributes, ProposerDuty};
use relay_gateway::{BeaconConnection, BeaconHandle, EventStream};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tracing::{error, info, warn};

enum InnerState {
    Connected(EventStream),
    Reconnecting(Pin<Box<dyn Future<Output = EventStream> + Send>>),
}

pub struct RelayService<S: Storage, C: BeaconConnection> {
    inner: InnerState,
    storage: Arc<S>,
    beacon_handle: BeaconHandle,
    beacon_client: Arc<C>,
    slots_per_epoch: u64,
}

impl<S: Storage, C: BeaconConnection> RelayService<S, C> {
    pub fn new(
        storage: Arc<S>,
        beacon_handle: BeaconHandle,
        beacon_client: Arc<C>,
        slots_per_epoch: u64,
        event_stream: EventStream,
    ) -> Self {
        Self {
            inner: InnerState::Connected(event_stream),
            storage,
            beacon_handle,
            beacon_client,
            slots_per_epoch,
        }
    }
}

fn into_duties<S: Storage>(storage: &S, duties: &[ProposerDuty]) -> Vec<ProposerDuty> {
    if storage.empty_validator_regs() {
        return vec![];
    }
    duties
        .iter()
        .filter(|duty| storage.read_validator_registration(&duty.pubkey).is_some())
        .cloned()
        .collect()
}

fn parse_payload_attributes(
    event: &relay_entity::PayloadAttributesEvent,
) -> Option<PayloadAttributes> {
    let inner = &event.data.payload_attributes;
    let prev_randao = inner.prev_randao.parse().ok()?;
    let suggested_fee_recipient = inner.suggested_fee_recipient.parse().ok()?;
    Some(PayloadAttributes {
        timestamp: inner.timestamp,
        prev_randao,
        suggested_fee_recipient,
    })
}

impl<S: Storage + 'static, C: BeaconConnection + 'static> Future for RelayService<S, C> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &mut self.inner {
                InnerState::Connected(stream) => match stream.poll_next_unpin(cx) {
                    Poll::Ready(Some(Ok(BeaconEvent::Head(head)))) => {
                        let slot = HeadSlot::from(head.slot);
                        info!(target: "relay_service", slot = head.slot, "head slot updated");
                        self.storage.set_head_slot(slot);

                        if slot.is_duty_refresh_slot(self.slots_per_epoch) {
                            let epoch = slot.epoch(self.slots_per_epoch);
                            info!(target: "relay_service", epoch, "refreshing proposer duties");
                            let handle = self.beacon_handle.clone();
                            let storage = self.storage.clone();
                            tokio::spawn(async move {
                                match handle.proposer_duties(epoch).await {
                                    Ok(duties) => {
                                        let filtered = into_duties(&*storage, &duties);
                                        if !filtered.is_empty() {
                                            info!(target: "relay_service", count = filtered.len(), "duties refreshed");
                                            storage.set_proposer_duties(filtered);
                                        } else {
                                            warn!(target: "relay_service", "no registered validators for duties");
                                        }
                                    }
                                    Err(e) => {
                                        error!(target: "relay_service", error = %e, "failed to fetch proposer duties");
                                    }
                                }
                            });
                        }
                        continue;
                    }
                    Poll::Ready(Some(Ok(BeaconEvent::PayloadAttributes(pa_event)))) => {
                        match parse_payload_attributes(&pa_event) {
                            Some(attrs) => {
                                info!(target: "relay_service", slot = pa_event.data.proposal_slot, "payload attributes updated");
                                self.storage.set_payload_attributes(attrs);
                            }
                            None => {
                                error!(target: "relay_service", "failed to parse payload attributes event");
                            }
                        }
                        continue;
                    }
                    Poll::Ready(Some(Err(err))) => {
                        error!(target: "relay_service", error = %err, "beacon event stream error, reconnecting");
                        let client = self.beacon_client.clone();
                        self.inner =
                            InnerState::Reconnecting(Box::pin(
                                async move { client.stream().await },
                            ));
                        continue;
                    }
                    Poll::Ready(None) => {
                        info!(target: "relay_service", "beacon event stream ended, reconnecting");
                        let client = self.beacon_client.clone();
                        self.inner =
                            InnerState::Reconnecting(Box::pin(
                                async move { client.stream().await },
                            ));
                        continue;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                InnerState::Reconnecting(fut) => match fut.as_mut().poll(cx) {
                    Poll::Ready(stream) => {
                        info!(target: "relay_service", "reconnected to beacon events");
                        self.inner = InnerState::Connected(stream);
                        continue;
                    }
                    Poll::Pending => return Poll::Pending,
                },
            }
        }
    }
}
