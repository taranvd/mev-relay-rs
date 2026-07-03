use serde::{Deserialize, Serialize};

/// The head slot of the latest block.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct HeadSlot(pub u64);

impl HeadSlot {
    /// Ensures the proposal slot is the next slot.
    pub fn is_next_slot(&self, proposal_slot: u64) -> bool {
        self.0 + 1 == proposal_slot
    }

    /// Returns true if the slot is the designated slot to refresh proposer duties.
    /// We intentionally refresh on the second slot of an epoch (1, 33, 65, 97...)
    /// to give the beacon node enough time to calculate duties for the next epoch.
    pub fn is_duty_refresh_slot(&self, slots_per_epoch: u64) -> bool {
        self.0 % slots_per_epoch == 1
    }
    /// Returns the epoch number of the event from the slot number.
    pub fn epoch(&self, slots_per_epoch: u64) -> u64 {
        self.0 / slots_per_epoch
    }
}

impl From<u64> for HeadSlot {
    fn from(slot: u64) -> Self {
        Self(slot)
    }
}

impl AsRef<u64> for HeadSlot {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_duty_refresh_slot_true() {
        let slots_per_epoch = 32;
        let slot = HeadSlot::from(33);
        assert!(slot.is_duty_refresh_slot(slots_per_epoch));
    }

    #[test]
    fn test_is_duty_refresh_slot_false() {
        let slots_per_epoch = 32;
        let slot = HeadSlot::from(34); // Not the refresh slot
        assert!(!slot.is_duty_refresh_slot(slots_per_epoch));
    }

    #[test]
    fn test_epoch_calculation() {
        let slots_per_epoch = 32;
        let tests = vec![
            (32, 1), // Last slot of the first epoch
            (33, 1), // First slot of the second epoch
            (64, 2), // Last slot of the second epoch
            (65, 2), // First slot of the third epoch
        ];
        for (slot, expected_epoch) in tests {
            let slot = HeadSlot::from(slot);
            assert_eq!(slot.epoch(slots_per_epoch), expected_epoch);
        }
    }
}
