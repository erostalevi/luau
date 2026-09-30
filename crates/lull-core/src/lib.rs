//! Lull core: domain model, file store, search, history and integrations.
//!
//! Layout (hexagonal):
//! - pure domain: [`ids`], [`markdown`], [`json_fmt`], [`model`]
//! - application + adapters: store, search, history, integrations (added per module)

pub mod brand;
pub mod error;
pub mod fsutil;
pub mod ids;
pub mod json_fmt;
pub mod markdown;
pub mod model;
pub mod store;

pub use error::{Error, Result};
