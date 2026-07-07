use serde::{Deserialize, Serialize};

use crate::ForkName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Versioned<T> {
    pub version: ForkName,
    pub data: T,
}

impl<T> Versioned<T> {
    pub fn new(version: ForkName, data: T) -> Self {
        Self { version, data }
    }
}
