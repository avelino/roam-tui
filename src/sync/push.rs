use crate::api::client::RoamClient;
use crate::error::{Result, RoamError};

use super::store::SyncStore;
use super::{SyncConfig, SyncReport};

pub async fn push_sync(
    _client: &RoamClient,
    _store: &SyncStore,
    _config: &SyncConfig,
) -> Result<SyncReport> {
    Err(RoamError::Generic(
        "Push sync is not yet implemented. Coming in Phase 2.".to_string(),
    ))
}
