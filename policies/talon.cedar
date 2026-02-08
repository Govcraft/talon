// ============================================================================
// Talon Gateway Cedar Policies
// ============================================================================
//
// These policies control access to the Talon AI gateway API.
// Cedar evaluates requests as (principal, action, resource) triples.
//
// Principals: authenticated users with roles and trust tiers
// Actions:    HTTP method + path patterns (e.g. "POST /api/v1/chat")
// Resources:  the target resource (tenant, agent, etc.)
//
// NOTE: Cedar is disabled by default in config.toml (enabled = false).
//       Enable it when authentication is wired up.
// ============================================================================

// ---------------------------------------------------------------------------
// Chat access -- any authenticated principal may send chat requests
// ---------------------------------------------------------------------------
permit(
    principal,
    action == Action::"POST /api/v1/chat",
    resource
);

permit(
    principal,
    action == Action::"POST /api/v1/chat/stream",
    resource
);

// ---------------------------------------------------------------------------
// Session read access -- any authenticated principal may list/get sessions
// ---------------------------------------------------------------------------
permit(
    principal,
    action == Action::"GET /api/v1/sessions",
    resource
);

permit(
    principal,
    action == Action::"GET /api/v1/sessions/{id}",
    resource
);

// ---------------------------------------------------------------------------
// Tenant management -- only principals with the "admin" role
// ---------------------------------------------------------------------------
permit(
    principal,
    action == Action::"POST /api/v1/tenants",
    resource
) when {
    principal.roles.contains("admin")
};

permit(
    principal,
    action == Action::"GET /api/v1/tenants",
    resource
) when {
    principal.roles.contains("admin")
};

permit(
    principal,
    action == Action::"GET /api/v1/tenants/{id}",
    resource
) when {
    principal.roles.contains("admin")
};

permit(
    principal,
    action == Action::"PUT /api/v1/tenants/{id}",
    resource
) when {
    principal.roles.contains("admin")
};

permit(
    principal,
    action == Action::"DELETE /api/v1/tenants/{id}",
    resource
) when {
    principal.roles.contains("admin")
};

// ---------------------------------------------------------------------------
// Agent management -- admin or tenant owner
// ---------------------------------------------------------------------------
permit(
    principal,
    action == Action::"POST /api/v1/tenants/{id}/agents",
    resource
) when {
    principal.roles.contains("admin") || principal.roles.contains("tenant_owner")
};

permit(
    principal,
    action == Action::"GET /api/v1/tenants/{id}/agents",
    resource
) when {
    principal.roles.contains("admin") || principal.roles.contains("tenant_owner")
};

permit(
    principal,
    action == Action::"PUT /api/v1/tenants/{id}/agents/{id}",
    resource
) when {
    principal.roles.contains("admin") || principal.roles.contains("tenant_owner")
};

permit(
    principal,
    action == Action::"DELETE /api/v1/tenants/{id}/agents/{id}",
    resource
) when {
    principal.roles.contains("admin") || principal.roles.contains("tenant_owner")
};
