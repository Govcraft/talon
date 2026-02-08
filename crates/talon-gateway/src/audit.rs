//! Audit logging helpers for Talon gateway domain events.
//!
//! These functions wrap `AuditLogger::log_custom()` from acton-service to emit
//! structured audit events for tenant, agent, and chat operations. Each helper
//! accepts `Option<&AuditLogger>` so it gracefully no-ops when the audit
//! subsystem is not configured.

use acton_service::prelude::*;

/// Log a tenant lifecycle event (created, updated, deleted).
///
/// Metadata should contain at minimum the tenant name and slug.
pub async fn log_tenant_event(
    logger: Option<&AuditLogger>,
    kind: &str,
    tenant_id: &str,
    metadata: serde_json::Value,
) {
    let Some(logger) = logger else { return };

    let mut meta = metadata;
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "tenant_id".into(),
            serde_json::Value::String(tenant_id.into()),
        );
    }

    logger
        .log_custom(
            &format!("tenant.{kind}"),
            AuditSeverity::Informational,
            Some(meta),
        )
        .await;
}

/// Log an agent lifecycle event (created, updated, deleted).
pub async fn log_agent_event(
    logger: Option<&AuditLogger>,
    kind: &str,
    tenant_id: &str,
    agent_id: &str,
    metadata: serde_json::Value,
) {
    let Some(logger) = logger else { return };

    let mut meta = metadata;
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "tenant_id".into(),
            serde_json::Value::String(tenant_id.into()),
        );
        obj.insert(
            "agent_id".into(),
            serde_json::Value::String(agent_id.into()),
        );
    }

    logger
        .log_custom(
            &format!("agent.{kind}"),
            AuditSeverity::Informational,
            Some(meta),
        )
        .await;
}

/// Log a chat request event.
pub async fn log_chat_event(
    logger: Option<&AuditLogger>,
    tenant_id: &str,
    sender_id: &str,
    model: &str,
) {
    let Some(logger) = logger else { return };

    logger
        .log_custom(
            "chat.request",
            AuditSeverity::Informational,
            Some(serde_json::json!({
                "tenant_id": tenant_id,
                "sender_id": sender_id,
                "model": model,
            })),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that passing `None` for the logger does not panic.
    #[tokio::test]
    async fn test_log_tenant_event_none_logger() {
        log_tenant_event(
            None,
            "created",
            "test-tenant",
            serde_json::json!({"name": "Test"}),
        )
        .await;
    }

    #[tokio::test]
    async fn test_log_agent_event_none_logger() {
        log_agent_event(
            None,
            "created",
            "test-tenant",
            "test-agent",
            serde_json::json!({"name": "Agent"}),
        )
        .await;
    }

    #[tokio::test]
    async fn test_log_chat_event_none_logger() {
        log_chat_event(None, "test-tenant", "user-1", "llama3.2").await;
    }
}
