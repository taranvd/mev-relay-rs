use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct HeadSlot(pub u64);

impl HeadSlot {
    pub fn is_next_slot(&self, slot: u64) -> bool {
        self.0 + 1 == slot
    }
}
