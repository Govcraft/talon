//! HTTP client wrapper for the Talon gateway REST API.
//!
//! All admin dashboard data flows through this client, which calls the
//! gateway at its `/api/v1/` endpoints using reqwest.

use reqwest::Client;
use talon_types::{
    Agent, CreateAgentRequest, CreateTenantRequest, Session, Tenant, UpdateTenantRequest,
};

/// Client for the Talon gateway HTTP API.
#[derive(Debug, Clone)]
pub struct GatewayApiClient {
    client: Client,
    base_url: String,
}

impl GatewayApiClient {
    /// Create a new client pointing at the given gateway base URL.
    ///
    /// The `base_url` should be the root of the gateway, e.g.
    /// `http://localhost:3000`. The `/api/v1` prefix is appended
    /// automatically by each method.
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    // -------------------------------------------------------------------
    // Tenants
    // -------------------------------------------------------------------

    /// List all tenants.
    #[tracing::instrument(skip(self))]
    pub async fn list_tenants(&self) -> anyhow::Result<Vec<Tenant>> {
        let url = format!("{}/api/v1/tenants", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let tenants = resp.json::<Vec<Tenant>>().await?;
        Ok(tenants)
    }

    /// Fetch a single tenant by ID.
    #[tracing::instrument(skip(self))]
    pub async fn get_tenant(&self, id: &str) -> anyhow::Result<Tenant> {
        let url = format!("{}/api/v1/tenants/{}", self.base_url, id);
        let resp = self.client.get(&url).send().await?.error_for_status()?;
        let tenant = resp.json::<Tenant>().await?;
        Ok(tenant)
    }

    /// Create a new tenant.
    #[tracing::instrument(skip(self))]
    pub async fn create_tenant(&self, req: &CreateTenantRequest) -> anyhow::Result<Tenant> {
        let url = format!("{}/api/v1/tenants", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await?
            .error_for_status()?;
        let tenant = resp.json::<Tenant>().await?;
        Ok(tenant)
    }

    /// Update an existing tenant.
    #[tracing::instrument(skip(self))]
    pub async fn update_tenant(
        &self,
        id: &str,
        req: &UpdateTenantRequest,
    ) -> anyhow::Result<Tenant> {
        let url = format!("{}/api/v1/tenants/{}", self.base_url, id);
        let resp = self
            .client
            .put(&url)
            .json(req)
            .send()
            .await?
            .error_for_status()?;
        let tenant = resp.json::<Tenant>().await?;
        Ok(tenant)
    }

    /// Delete a tenant by ID.
    #[tracing::instrument(skip(self))]
    pub async fn delete_tenant(&self, id: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/tenants/{}", self.base_url, id);
        self.client.delete(&url).send().await?.error_for_status()?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Agents (scoped to a tenant)
    // -------------------------------------------------------------------

    /// List agents belonging to a tenant.
    #[tracing::instrument(skip(self))]
    pub async fn list_agents(&self, tenant_id: &str) -> anyhow::Result<Vec<Agent>> {
        let url = format!("{}/api/v1/tenants/{}/agents", self.base_url, tenant_id);
        let resp = self.client.get(&url).send().await?;
        let agents = resp.json::<Vec<Agent>>().await?;
        Ok(agents)
    }

    /// Create a new agent for a tenant.
    #[tracing::instrument(skip(self))]
    pub async fn create_agent(
        &self,
        tenant_id: &str,
        req: &CreateAgentRequest,
    ) -> anyhow::Result<Agent> {
        let url = format!("{}/api/v1/tenants/{}/agents", self.base_url, tenant_id);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await?
            .error_for_status()?;
        let agent = resp.json::<Agent>().await?;
        Ok(agent)
    }

    /// Delete an agent by ID.
    #[tracing::instrument(skip(self))]
    pub async fn delete_agent(&self, tenant_id: &str, agent_id: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/tenants/{}/agents/{}",
            self.base_url, tenant_id, agent_id
        );
        self.client.delete(&url).send().await?.error_for_status()?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Sessions
    // -------------------------------------------------------------------

    /// List all sessions.
    #[tracing::instrument(skip(self))]
    pub async fn list_sessions(&self) -> anyhow::Result<Vec<Session>> {
        let url = format!("{}/api/v1/sessions", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let sessions = resp.json::<Vec<Session>>().await?;
        Ok(sessions)
    }

    /// Fetch a single session by ID.
    #[tracing::instrument(skip(self))]
    pub async fn get_session(&self, id: &str) -> anyhow::Result<Session> {
        let url = format!("{}/api/v1/sessions/{}", self.base_url, id);
        let resp = self.client.get(&url).send().await?.error_for_status()?;
        let session = resp.json::<Session>().await?;
        Ok(session)
    }
}
