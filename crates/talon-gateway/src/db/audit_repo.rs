//! Tenant-scoped audit event repository.
//!
//! Stores audit events in the tenant namespace's `audit_event` table with
//! a BLAKE3 hash chain for tamper detection. This is separate from the
//! global audit trail managed by acton-service's `AuditAgent`.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::DbClient;
use crate::error::GatewayError;

/// A tenant-scoped audit event record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantAuditEvent {
    pub id: String,
    pub event_type: String,
    pub severity: String,
    pub metadata: Option<serde_json::Value>,
    pub hash: String,
    pub previous_hash: Option<String>,
    pub sequence: u64,
    pub timestamp: String,
}

/// Row representation for SurrealDB deserialisation.
#[derive(Debug, Deserialize)]
struct AuditEventRow {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    event_type: String,
    severity: String,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    hash: String,
    #[serde(default)]
    previous_hash: Option<String>,
    #[serde(default)]
    sequence: u64,
    timestamp: String,
}

impl AuditEventRow {
    fn into_event(self) -> TenantAuditEvent {
        let raw_id = self.id.id.to_raw();
        TenantAuditEvent {
            id: raw_id,
            event_type: self.event_type,
            severity: self.severity,
            metadata: self.metadata,
            hash: self.hash,
            previous_hash: self.previous_hash,
            sequence: self.sequence,
            timestamp: self.timestamp,
        }
    }
}

/// Compute a BLAKE3 hash for a tenant audit event, chained to the previous hash.
fn compute_event_hash(
    sequence: u64,
    previous_hash: Option<&str>,
    event_type: &str,
    severity: &str,
    timestamp: &str,
    metadata: Option<&serde_json::Value>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(sequence.to_le_bytes().as_ref());
    if let Some(prev) = previous_hash {
        hasher.update(prev.as_bytes());
    }
    hasher.update(event_type.as_bytes());
    hasher.update(severity.as_bytes());
    hasher.update(timestamp.as_bytes());
    if let Some(meta) = metadata {
        hasher.update(meta.to_string().as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Append an audit event to the tenant's audit chain.
///
/// Retrieves the latest event to obtain the previous hash and sequence,
/// then inserts a new event with the computed BLAKE3 hash.
#[tracing::instrument(skip(client))]
pub async fn append_audit_event(
    client: &DbClient,
    tenant_ns: &str,
    event_type: &str,
    severity: &str,
    metadata: Option<serde_json::Value>,
) -> std::result::Result<TenantAuditEvent, GatewayError> {
    // Fetch the latest event to get the chain tip.
    let latest_query = format!(
        "USE NS `{tenant_ns}` DB main; \
         SELECT * FROM audit_event ORDER BY sequence DESC LIMIT 1"
    );
    let mut result = client
        .query(&latest_query)
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let latest_rows: Vec<AuditEventRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let (prev_hash, prev_seq) = latest_rows
        .into_iter()
        .next()
        .map(|row| (Some(row.hash), row.sequence))
        .unwrap_or((None, 0));

    let sequence = prev_seq + 1;
    let timestamp = Utc::now().to_rfc3339();
    let hash = compute_event_hash(
        sequence,
        prev_hash.as_deref(),
        event_type,
        severity,
        &timestamp,
        metadata.as_ref(),
    );

    let event_id = ulid::Ulid::new().to_string().to_lowercase();
    let record = serde_json::json!({
        "id": event_id,
        "event_type": event_type,
        "severity": severity,
        "metadata": metadata,
        "hash": hash,
        "previous_hash": prev_hash,
        "sequence": sequence,
        "timestamp": timestamp,
    });

    let insert_query = format!(
        "USE NS `{tenant_ns}` DB main; \
         CREATE type::thing('audit_event', $id) CONTENT $data"
    );
    let mut insert_result = client
        .query(&insert_query)
        .bind(("id", event_id.clone()))
        .bind(("data", record))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AuditEventRow> = insert_result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    rows.into_iter()
        .next()
        .map(AuditEventRow::into_event)
        .ok_or_else(|| GatewayError::Internal("failed to create tenant audit event".into()))
}

/// List audit events for a tenant namespace, ordered by sequence.
#[tracing::instrument(skip(client))]
pub async fn list_audit_events(
    client: &DbClient,
    tenant_ns: &str,
    limit: u32,
) -> std::result::Result<Vec<TenantAuditEvent>, GatewayError> {
    let query = format!(
        "USE NS `{tenant_ns}` DB main; \
         SELECT * FROM audit_event ORDER BY sequence ASC LIMIT {limit}"
    );
    let mut result = client
        .query(&query)
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AuditEventRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(AuditEventRow::into_event).collect())
}

/// Verify the integrity of a tenant's audit chain by recomputing hashes.
///
/// Returns `Ok(())` if the chain is intact, or an error describing the
/// first broken link.
#[tracing::instrument(skip(events))]
pub fn verify_tenant_audit_chain(
    events: &[TenantAuditEvent],
) -> std::result::Result<(), GatewayError> {
    let mut expected_prev: Option<&str> = None;

    for event in events {
        // Check previous_hash linkage.
        let actual_prev = event.previous_hash.as_deref();
        if actual_prev != expected_prev {
            return Err(GatewayError::Internal(format!(
                "audit chain broken at sequence {}: expected previous_hash {:?}, got {:?}",
                event.sequence, expected_prev, actual_prev
            )));
        }

        // Recompute hash.
        let recomputed = compute_event_hash(
            event.sequence,
            event.previous_hash.as_deref(),
            &event.event_type,
            &event.severity,
            &event.timestamp,
            event.metadata.as_ref(),
        );
        if recomputed != event.hash {
            return Err(GatewayError::Internal(format!(
                "audit chain tampered at sequence {}: hash mismatch",
                event.sequence
            )));
        }

        expected_prev = Some(event.hash.as_str());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_event_hash_deterministic() {
        let h1 = compute_event_hash(
            1,
            None,
            "tenant.created",
            "informational",
            "2026-01-01T00:00:00Z",
            None,
        );
        let h2 = compute_event_hash(
            1,
            None,
            "tenant.created",
            "informational",
            "2026-01-01T00:00:00Z",
            None,
        );
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_event_hash_changes_with_sequence() {
        let h1 = compute_event_hash(
            1,
            None,
            "tenant.created",
            "informational",
            "2026-01-01T00:00:00Z",
            None,
        );
        let h2 = compute_event_hash(
            2,
            None,
            "tenant.created",
            "informational",
            "2026-01-01T00:00:00Z",
            None,
        );
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compute_event_hash_chains() {
        let h1 = compute_event_hash(
            1,
            None,
            "tenant.created",
            "informational",
            "2026-01-01T00:00:00Z",
            None,
        );
        let h2 = compute_event_hash(
            2,
            Some(&h1),
            "agent.created",
            "informational",
            "2026-01-01T00:00:01Z",
            None,
        );
        // Changing the previous hash changes the output.
        let h2_alt = compute_event_hash(
            2,
            None,
            "agent.created",
            "informational",
            "2026-01-01T00:00:01Z",
            None,
        );
        assert_ne!(h2, h2_alt);
    }

    #[test]
    fn test_verify_empty_chain() {
        assert!(verify_tenant_audit_chain(&[]).is_ok());
    }

    #[test]
    fn test_verify_valid_chain() {
        let h1 = compute_event_hash(1, None, "tenant.created", "informational", "t1", None);
        let h2 = compute_event_hash(2, Some(&h1), "agent.created", "informational", "t2", None);

        let events = vec![
            TenantAuditEvent {
                id: "e1".into(),
                event_type: "tenant.created".into(),
                severity: "informational".into(),
                metadata: None,
                hash: h1.clone(),
                previous_hash: None,
                sequence: 1,
                timestamp: "t1".into(),
            },
            TenantAuditEvent {
                id: "e2".into(),
                event_type: "agent.created".into(),
                severity: "informational".into(),
                metadata: None,
                hash: h2,
                previous_hash: Some(h1),
                sequence: 2,
                timestamp: "t2".into(),
            },
        ];

        assert!(verify_tenant_audit_chain(&events).is_ok());
    }

    #[test]
    fn test_verify_tampered_chain() {
        let h1 = compute_event_hash(1, None, "tenant.created", "informational", "t1", None);

        let events = vec![TenantAuditEvent {
            id: "e1".into(),
            event_type: "tenant.created".into(),
            severity: "informational".into(),
            metadata: None,
            hash: "tampered".into(),
            previous_hash: None,
            sequence: 1,
            timestamp: "t1".into(),
        }];

        let _ = h1; // suppress unused warning
        assert!(verify_tenant_audit_chain(&events).is_err());
    }
}
