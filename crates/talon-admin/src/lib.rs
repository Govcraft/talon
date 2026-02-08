//! Admin dashboard for the Talon AI gateway.
//!
//! Provides an HTMX + Askama server-rendered admin UI that communicates
//! with the gateway's HTTP API. All data is fetched via reqwest -- the
//! admin service never touches SurrealDB directly.

pub mod api_client;
pub mod handlers;
pub mod routes;
