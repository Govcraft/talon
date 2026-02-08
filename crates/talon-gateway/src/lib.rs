//! AI inference gateway service for Talon.
//!
//! This crate provides the HTTP-facing gateway that accepts chat requests,
//! manages conversation sessions, and delegates inference to acton-ai.

pub mod agent_handlers;
pub mod audit;
pub mod circuit_breaker;
pub mod db;
pub mod error;
pub mod grpc_service;
pub mod handlers;
pub mod inference;
pub mod rate_limit;
pub mod routes;
pub mod session_store;
pub mod tenant_handlers;
