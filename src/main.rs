//! `test-app-xpub` — server-rendered Axum web app exercising
//! `asterism-xpub`.
//!
//! Boot sequence:
//! 1. Load `.env`, build [`AppConfig`].
//! 2. Connect to PostgreSQL.
//! 3. Run `migrations/*.sql` (domain schema).
//! 4. Initialise `tower-sessions` Postgres store + run its own schema migration.
//! 5. Seed the three test users (idempotent).
//! 6. Build the router and serve on `APP_HOST:APP_PORT`.

mod auth;
mod config;
mod db;
mod error;
mod handlers;
mod models;
mod wallet;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::Router;
use axum::routing::{get, post};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::Duration as TimeDuration;
use tokio::signal;
use tokio::task::AbortHandle;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::Key;
use tower_sessions::session_store::ExpiredDeletion;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::config::AppConfig;
use crate::wallet::WalletManager;

/// Application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration loaded at startup.
    pub config: AppConfig,
    /// Shared PostgreSQL connection pool.
    pub db: PgPool,
    /// Per-federation BDK wallet cache + Bitcoin Core RPC client.
    pub wallets: Arc<WalletManager>,
}

// Boot sequence is linear and well-commented; splitting just to satisfy the
// 100-line cap would obscure the relative ordering of `migrate → seed →
// session-store init → router build`.
#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load `.env` next to the crate root if present (so the app can be
    // started from any CWD). Missing file is fine: callers can use the
    // process environment directly.
    let env_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    let _ = dotenvy::from_path(env_path);

    init_tracing();

    let config = AppConfig::from_env()?;
    tracing::info!(
        bind = %config.bind,
        network = %config.network,
        derivation = %config.federation_derivation_path,
        "starting test-app-xpub"
    );

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(StdDuration::from_secs(8))
        .connect(&config.database_url)
        .await?;

    db::migrate(&pool).await?;
    tracing::info!("domain schema migrated");

    auth::seed_test_users(&pool).await?;
    tracing::info!("test users seeded");

    let session_store = PostgresStore::new(pool.clone());
    session_store.migrate().await?;

    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(StdDuration::from_secs(60)),
    );

    let cookie_key = Key::try_from(config.session_secret.as_slice())
        .map_err(|e| format!("APP_SESSION_SECRET cannot be used as cookie signing key: {e}"))?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_signed(cookie_key)
        .with_secure(false) // dev: served over plain HTTP on localhost
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(TimeDuration::days(7)))
        .with_name("asterism_session");

    let wallets = Arc::new(WalletManager::new(pool.clone(), &config)?);
    tracing::info!(
        rpc = %config.bitcoin_rpc_url,
        wallet = %config.bitcoin_wallet_name,
        "Bitcoin Core RPC client ready",
    );

    let state = Arc::new(AppState {
        config: config.clone(),
        db: pool,
        wallets,
    });

    // Resolve `static/` relative to the crate root so the binary works
    // regardless of the caller's CWD.
    let static_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");

    let app = Router::new()
        .route("/", get(handlers::home::root))
        .route("/home", get(handlers::home::home))
        .route(
            "/login",
            get(handlers::auth::login_get).post(handlers::auth::login_post),
        )
        .route("/logout", post(handlers::auth::logout_post))
        .route("/onboard", get(handlers::onboard::onboard_get))
        .route(
            "/onboard/signer",
            post(handlers::onboard::onboard_signer_post),
        )
        .route(
            "/federations/{id}",
            get(handlers::federations::redirect_to_default),
        )
        .route(
            "/federations/{id}/receive",
            get(handlers::federations::receive),
        )
        .route("/federations/{id}/send", get(handlers::federations::send))
        .route(
            "/federations/{id}/addresses/{address}",
            get(handlers::addresses::show),
        )
        .route(
            "/federations/{id}/proposals",
            post(handlers::proposals::create),
        )
        .route(
            "/federations/{id}/proposals/{pid}",
            get(handlers::proposals::detail),
        )
        .route(
            "/federations/{id}/proposals/{pid}/sign-data",
            get(handlers::proposals::sign_data),
        )
        .route(
            "/federations/{id}/proposals/{pid}/signatures",
            post(handlers::proposals::submit_signature),
        )
        .route(
            "/federations/{id}/proposals/{pid}/rejections",
            post(handlers::proposals::submit_rejection),
        )
        .route(
            "/federations/{id}/proposals/{pid}/cancel",
            post(handlers::proposals::cancel),
        )
        .route(
            "/federations/{id}/proposals/{pid}/broadcast",
            post(handlers::proposals::broadcast),
        )
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(TraceLayer::new_for_http())
        .layer(session_layer)
        .with_state(state);

    let addr: SocketAddr = config.bind;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal(deletion_task.abort_handle()))
        .await?;

    // We deliberately abort the deletion task on shutdown; treat the resulting
    // `JoinError::Cancelled` as a clean exit.
    match deletion_task.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "session-deletion task ended with error"),
        Err(e) if e.is_cancelled() => {}
        Err(e) => tracing::warn!(error = %e, "session-deletion task join error"),
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("test_app_xpub=debug,tower_http=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}

async fn shutdown_signal(deletion_abort: AbortHandle) {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install ctrl_c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
    deletion_abort.abort();
}
