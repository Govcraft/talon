//! TalonHub Registry Service
//!
//! HTTP service for skill discovery, publishing, and verification.
//! This is a proprietary component of the Talon ecosystem.

pub mod error;
pub mod handlers;
pub mod models;
pub mod routes;

pub use error::{RegistryError, RegistryResult};
