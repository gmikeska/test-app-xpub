//! Shared library for `test-app-xpub`.
//!
//! The web-server binary lives in `main.rs`; example/developer tools
//! (`examples/*.rs`) import these modules through this library crate — e.g.
//! `examples/rescan_federation.rs` reuses [`wallet::WalletManager::rescan`].
//!
//! These modules are app-internal surface exposed for the binary + examples,
//! not a published crate API, so the doc-rigor pedantic lints are relaxed
//! (mirrors `test-app-pkcs11`'s lib).

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions
)]

use std::sync::Arc;

use sqlx::PgPool;

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod jade;
pub mod models;
pub mod wallet;

use crate::config::AppConfig;
use crate::wallet::WalletManager;

/// Application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration loaded at startup.
    pub config: AppConfig,
    /// Shared `PostgreSQL` connection pool.
    pub db: PgPool,
    /// Per-federation BDK wallet cache + Bitcoin Core RPC client.
    pub wallets: Arc<WalletManager>,
}
