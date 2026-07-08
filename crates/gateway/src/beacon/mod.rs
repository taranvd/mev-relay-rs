pub mod api;
pub mod error;
pub mod events;
pub mod service;

pub use api::{BeaconApi, BeaconNodeApi};
pub use error::BeaconError;
pub use events::BeaconConnection;
pub use events::BeaconEventsClient;
pub use service::BeaconService;
pub use service::handle::BeaconHandle;
