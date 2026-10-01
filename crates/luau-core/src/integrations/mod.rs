//! Remote issue providers (Jira Cloud / Data Center, Trello), mirror boards,
//! linked cards, Slack and the WriteGate (SPEC §9).
//!
//! Layout (hexagonal):
//! - pure domain: [`types`], [`adf`], [`wiki`], [`md_util`], [`links`] (format), [`mirror`] (planning), [`gate`],
//!   [`query`] (JQL / plain-text search building)
//! - adapters: [`http`] (allow-list, backoff), [`jira`], [`trello`], [`slack`], [`secrets`] (keychain)
//! - port: [`provider::Provider`]; application layer: [`service`]

pub mod accounts;
pub mod adf;
pub mod gate;
pub mod http;
pub mod jira;
pub mod links;
pub mod md_util;
pub mod mirror;
pub mod provider;
pub mod query;
pub mod secrets;
pub mod service;
pub mod slack;
pub mod trello;
pub mod types;
pub mod wiki;

pub use service::start_watcher;
