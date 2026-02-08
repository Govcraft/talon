//! Usage tracking types for the Talon AI gateway.

use serde::{Deserialize, Serialize};

use crate::TenantId;

/// Usage record tracking token consumption per tenant/period/model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub tenant_id: TenantId,
    /// Period string in `YYYY-MM` format.
    pub period: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub request_count: u64,
}
