use futures::StreamExt;
use relay_datastore::Storage;
use relay_entity::{
    BeaconEvent, HeadEvent, HeadSlot, PayloadAttributes, PayloadAttributesEvent, ProposerDuty,
};
use relay_gateway::{BeaconHandle, BeaconNodeApi};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

pub struct RelayService {
    bn_client: BeaconNodeApi,
    storage: Arc<dyn Storage>,
    beacon_handle: BeaconHandle,
    slots_per_epoch: u64,
}

impl RelayService {
    pub fn new(
        bn_client: BeaconNodeApi,
        storage: Arc<dyn Storage>,
        beacon_handle: BeaconHandle,
        slots_per_epoch: u64,
    ) -> Self {
        Self {
            bn_client,
            storage,
            beacon_handle,
            slots_per_epoch,
        }
    }

    pub async fn run(self) {
        info!("relay service started");

        loop {
            match self.connect_sse().await {
                Ok(()) => {
                    warn!("SSE disconnected, reconnecting in 5s...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    warn!(error = %e, "SSE failed, falling back to polling");
                    self.polling_loop().await;
                }
            }
        }
    }

    async fn connect_sse(&self) -> Result<(), String> {
        let response = self
            .bn_client
            .subscribe_events()
            .await
            .map_err(|e| e.to_string())?;
        let mut byte_stream = response.bytes_stream();
        let mut line_buf = String::new();
        let mut current_event: Option<String> = None;

        while let Some(chunk) = byte_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    let s = String::from_utf8_lossy(&bytes);
                    line_buf.push_str(&s);
                    while let Some(pos) = line_buf.find('\n') {
                        let line = line_buf[..pos].to_string();
                        line_buf = line_buf[pos + 1..].to_string();
                        if let Some(event) = self.process_sse_line(&line, &mut current_event).await
                        {
                            self.handle_event(event).await;
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "SSE stream error");
                    return Err(e.to_string());
                }
            }
        }
        Err("SSE stream ended".into())
    }

    async fn process_sse_line(
        &self,
        line: &str,
        current_event: &mut Option<String>,
    ) -> Option<BeaconEvent> {
        if line.is_empty() || line.starts_with(':') {
            return None;
        }
        if let Some(event) = line.strip_prefix("event: ") {
            *current_event = Some(event.trim().to_string());
            return None;
        }
        if let Some(data) = line.strip_prefix("data: ") {
            let event_type = current_event.take()?;
            match event_type.as_str() {
                "head" => serde_json::from_str::<HeadEvent>(data)
                    .ok()
                    .map(BeaconEvent::Head),
                "payload_attributes" => serde_json::from_str::<PayloadAttributesEvent>(data)
                    .ok()
                    .map(BeaconEvent::PayloadAttributes),
                _ => {
                    warn!(target: "relay_service", event = event_type, "unknown event type");
                    None
                }
            }
        } else {
            None
        }
    }

    async fn handle_event(&self, event: BeaconEvent) {
        match event {
            BeaconEvent::Head(head) => self.handle_head_event(head).await,
            BeaconEvent::PayloadAttributes(pa) => {
                self.handle_payload_attributes_event(pa).await;
            }
        }
    }

    async fn handle_head_event(&self, event: HeadEvent) {
        let slot = HeadSlot(event.slot);
        self.storage.set_head_slot(slot);
        info!(target: "relay_service", slot = event.slot, block = %event.block, "head slot updated");

        if slot.is_duty_refresh_slot(self.slots_per_epoch) {
            let current_epoch = slot.epoch(self.slots_per_epoch);
            if let Err(e) = self.sync_duties(current_epoch).await {
                warn!(target: "relay_service", error = %e, "duty sync failed at head event");
            }
        }
    }

    async fn handle_payload_attributes_event(&self, event: PayloadAttributesEvent) {
        let inner = &event.data.payload_attributes;
        let prev_randao = match inner.prev_randao.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "relay_service", error = %e, "failed to parse prev_randao");
                return;
            }
        };
        let suggested_fee_recipient = match inner.suggested_fee_recipient.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "relay_service", error = %e, "failed to parse fee recipient");
                return;
            }
        };
        let attrs = PayloadAttributes {
            timestamp: inner.timestamp,
            prev_randao,
            suggested_fee_recipient,
        };
        self.storage.set_payload_attributes(attrs);
        info!(
            target: "relay_service",
            proposal_slot = event.data.proposal_slot,
            "payload attributes updated"
        );
    }

    async fn sync_duties(&self, epoch: u64) -> Result<(), String> {
        match self.beacon_handle.proposer_duties(epoch).await {
            Ok(duties) => {
                let filtered = self.filter_duties(&duties);
                let next_epoch_duties = self.beacon_handle.proposer_duties(epoch + 1).await;
                match next_epoch_duties {
                    Ok(next) => {
                        let mut all = filtered;
                        let next_filtered = self.filter_duties(&next);
                        all.extend(next_filtered);
                        self.storage.set_proposer_duties(all);
                    }
                    Err(e) => {
                        warn!(target: "relay_service", error = %e, "failed to fetch next epoch duties");
                        self.storage.set_proposer_duties(filtered);
                    }
                }
                info!(target: "relay_service", epoch, "proposer duties synced");
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn filter_duties(&self, duties: &[ProposerDuty]) -> Vec<ProposerDuty> {
        if self.storage.empty_validator_regs() {
            return vec![];
        }
        duties
            .iter()
            .filter(|duty| {
                self.storage
                    .read_validator_registration(&duty.pubkey)
                    .is_some()
            })
            .cloned()
            .collect()
    }

    async fn polling_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(6));
        loop {
            interval.tick().await;
            match self.bn_client.fetch_head_slot().await {
                Ok(slot) => {
                    let slot = HeadSlot(slot);
                    self.storage.set_head_slot(slot);
                    info!(target: "relay_service", slot = slot.0, "head slot updated (polling)");
                    let epoch = slot.epoch(self.slots_per_epoch);
                    if slot.is_duty_refresh_slot(self.slots_per_epoch)
                        && let Err(e) = self.sync_duties(epoch).await
                    {
                        warn!(target: "relay_service", error = %e, "duty sync failed in polling");
                    }
                }
                Err(e) => {
                    warn!(target: "relay_service", error = %e, "polling head slot failed");
                }
            }
        }
    }
}
