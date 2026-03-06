# talon-admin Implementation Plan

## Overview

Implement the `talon-admin` crate -- an HTMX + Askama server-rendered admin dashboard for the Talon AI gateway. This is a thin frontend that makes HTTP calls to the gateway REST API at `http://localhost:3000/api/v1/...` via reqwest.

## Architecture Decisions

1. **reqwest** as HTTP client to call the gateway API
2. **askama 0.13** for compile-time template rendering (standalone, not via acton-service features)
3. **acton-service** with features `http`, `sse`, `observability` for the web server
4. **NO SurrealDB access** - all data comes from gateway API
5. **NO session/auth** for now
6. Serves on port 8082 (gateway is 3000/8080, web channel is 8081)
7. Uses `VersionedApiBuilder` with base path "" so routes are under `/v1/`
8. Uses `Extension<Arc<GatewayApiClient>>` for shared state

## File Structure

```
crates/talon-admin/
  Cargo.toml               # Dependencies
  config.toml              # Service config (port 8082)
  src/
    main.rs                # Binary entry point
    lib.rs                 # Module declarations
    routes.rs              # Route definitions
    api_client.rs          # reqwest client wrapper for gateway API
    handlers/
      mod.rs               # Handler module declarations
      dashboard.rs         # GET / - dashboard overview
      tenants.rs           # Tenant CRUD pages
      agents.rs            # Agent management pages
      sessions.rs          # Session browser
  templates/
    base.html              # Base layout with nav, HTMX/Tailwind CDN includes
    dashboard.html         # Dashboard page
    tenants/
      list.html            # Tenant table
      detail.html          # Tenant detail view
      create.html          # Create tenant form
    agents/
      list.html            # Agent table for a tenant
      create.html          # Create agent form
    sessions/
      list.html            # Session list
    partials/
      nav.html             # Navigation sidebar
      tenant_row.html      # Single tenant table row (for HTMX swap)
      agent_row.html       # Single agent table row (for HTMX swap)
```

## Implementation Details

### Cargo.toml
- `acton-service = { version = "0.16", features = ["http", "sse", "observability"] }`
- `askama = "0.13"` (standalone, not acton-service feature)
- `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`
- `talon-types = { workspace = true }`
- Standard workspace deps: tokio, serde, serde_json, tracing, anyhow, chrono

### api_client.rs
- `GatewayApiClient` struct with `reqwest::Client` and `base_url: String`
- Methods for Tenants: list, get, create, update, delete
- Methods for Agents: list (per tenant), create (per tenant), delete (per tenant)
- Methods for Sessions: list, get
- All methods return `anyhow::Result<T>` where T is the domain type from talon-types
- URL construction: `{base_url}/api/v1/tenants`, etc.

### routes.rs
- `build_routes()` returns `VersionedRoutes`
- Uses `VersionedApiBuilder::new()` with `.with_base_path("")`
- All routes under ApiVersion::V1
- GatewayApiClient shared via `Extension<Arc<GatewayApiClient>>`
- Routes:
  - `GET /` -> dashboard::index
  - `GET /tenants` -> tenants::list
  - `POST /tenants` -> tenants::create
  - `GET /tenants/new` -> tenants::new_form
  - `GET /tenants/{id}` -> tenants::detail
  - `POST /tenants/{id}/delete` -> tenants::delete
  - `GET /tenants/{id}/agents` -> agents::list
  - `POST /tenants/{id}/agents` -> agents::create
  - `GET /tenants/{id}/agents/new` -> agents::new_form
  - `POST /tenants/{id}/agents/{agent_id}/delete` -> agents::delete
  - `GET /sessions` -> sessions::list

### Handler Pattern
- Each handler extracts `Extension<Arc<GatewayApiClient>>`
- Template structs use `#[derive(askama::Template)]` with `#[template(path = "...")]`
- Handlers return `std::result::Result<Html<String>, StatusCode>`
- Template rendering: `tmpl.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)`
- Form data via `axum::extract::Form` for create/update operations

### Templates
- **base.html**: Tailwind CSS CDN + HTMX 2.0.4 CDN, sidebar nav, main content block
- **dashboard.html**: Extends base, shows tenant count and session count cards
- **tenants/list.html**: Extends base, table with tenant rows, link to create
- **tenants/detail.html**: Extends base, tenant info with agents section
- **tenants/create.html**: Extends base, form to create tenant
- **agents/list.html**: Agent table within a tenant context
- **agents/create.html**: Form to create agent for a tenant
- **sessions/list.html**: Extends base, session table
- **partials/nav.html**: Sidebar navigation (included in base.html)
- **partials/tenant_row.html**: Single `<tr>` for HTMX partial swap
- **partials/agent_row.html**: Single `<tr>` for HTMX partial swap

### HTMX Interaction Patterns
- Delete tenant: `hx-post="/v1/tenants/{id}/delete" hx-confirm="Delete this tenant?" hx-target="closest tr" hx-swap="outerHTML"`
- Delete agent: `hx-post="/v1/tenants/{tid}/agents/{aid}/delete" hx-confirm="Delete this agent?" hx-target="closest tr" hx-swap="outerHTML"`
- Create forms: standard POST with redirect

### Configuration
- `config.toml` with `[service]` section: port = 8082, host = "0.0.0.0"
- `TALON_GATEWAY_URL` env var for gateway base URL (default: "http://localhost:3000")

## Verification Steps
1. `cargo check -p talon-admin` passes
2. `cargo clippy -p talon-admin -- -D warnings` passes clean

## Semver
- **Minor bump (0.1.0 -> 0.2.0)**: New crate with new functionality, backwards compatible with workspace
- Since this is a brand new crate starting at workspace version 0.1.0, no bump needed

## Dependencies on Other Crates
- `talon-types`: Tenant, Agent, Session, CreateTenantRequest, CreateAgentRequest, UpdateTenantRequest, Plan, TenantStatus, TrustTier, SessionStatus, TenantSettings, SessionKey
- `acton-service`: ServiceBuilder, VersionedApiBuilder, ApiVersion, VersionedRoutes, Extension, Html, StatusCode, etc.
