pub mod api;
pub mod error;

// Convenience re-exports
pub use api::client::RoamClient;
pub use api::queries;
pub use api::types;
pub use error::{Result, RoamError};
