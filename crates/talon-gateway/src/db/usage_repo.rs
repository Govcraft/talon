//! Usage tracking repository -- token consumption records per tenant namespace.

use chrono::Utc;
use serde::Deserialize;

use super::DbClient;
use crate::error::GatewayError;
use talon_types::{TenantId, UsageRecord};

/// Internal row for SurrealDB deserialisation.
#[derive(Debug, Deserialize)]
struct UsageRow {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    tenant_id: String,
    period: String,
    provider: String,
    model: String,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    request_count: u64,
}

impl UsageRow {
    fn into_usage(self) -> UsageRecord {
        UsageRecord {
            tenant_id: TenantId::new(self.tenant_id),
            period: self.period,
            provider: self.provider,
            model: self.model,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            request_count: self.request_count,
        }
    }
}

/// Increment usage counters for the current period.
///
/// Uses an upsert pattern: if a record for this (tenant, period, provider, model)
/// combination already exists, its counters are incremented; otherwise a new
/// record is created.
#[tracing::instrument(skip(client))]
pub async fn increment_usage(
    client: &DbClient,
    tenant_ns: &str,
    tenant_id: &TenantId,
    provider: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> std::result::Result<(), GatewayError> {
    let period = Utc::now().format("%Y-%m").to_string();

    // Use SurrealDB UPSERT to atomically create-or-update.
    let query = format!(
        "USE NS `{tenant_ns}` DB main; \
         UPSERT usage SET \
           tenant_id = $tenant_id, \
           period = $period, \
           provider = $provider, \
           model = $model, \
           input_tokens += $in_tokens, \
           output_tokens += $out_tokens, \
           request_count += 1 \
         WHERE tenant_id = $tenant_id \
           AND period = $period \
           AND provider = $provider \
           AND model = $model"
    );
    client
        .query(&query)
        .bind(("tenant_id", tenant_id.as_str().to_string()))
        .bind(("period", period))
        .bind(("provider", provider.to_string()))
        .bind(("model", model.to_string()))
        .bind(("in_tokens", input_tokens))
        .bind(("out_tokens", output_tokens))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(())
}

/// Get usage records for a tenant in a specific period.
#[tracing::instrument(skip(client))]
pub async fn get_usage(
    client: &DbClient,
    tenant_ns: &str,
    tenant_id: &TenantId,
    period: &str,
) -> std::result::Result<Vec<UsageRecord>, GatewayError> {
    let query = format!(
        "USE NS `{tenant_ns}` DB main; \
         SELECT * FROM usage \
         WHERE tenant_id = $tenant_id AND period = $period"
    );
    let mut result = client
        .query(&query)
        .bind(("tenant_id", tenant_id.as_str().to_string()))
        .bind(("period", period.to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<UsageRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(UsageRow::into_usage).collect())
}
