//! Luau core: domain model, file store, search, history and integrations.
#![allow(clippy::type_complexity)]
//!
//! Layout (hexagonal):
//! - pure domain: [`ids`], [`markdown`], [`json_fmt`], [`model`]
//! - application + adapters: store, search, history, integrations (added per module)

pub mod ai;
pub mod app;
pub mod brand;
pub mod discovery;
pub mod error;
pub mod fsutil;
pub mod history;
pub mod ids;
pub mod integrations;
pub mod io;
pub mod json_fmt;
pub mod markdown;
pub mod model;
pub mod search;
pub mod store;

pub use error::{Error, Result};
