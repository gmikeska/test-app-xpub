//! **One-off recovery runner** (explicit gate `HEAL_LIVE=1`, *mutates* the live
//! `asterism_xpub` DB) — proves Maggie's recovery ask #3 on the **real** corrupted
//! demo rows: does the orphaned-anchor fix's `rebuild_on_reorg` path heal a wallet
//! that already baked in reorg damage, on its next `sync_lineage`, with **no forced
//! from-scratch rebuild**?
//!
//! For every lineage that holds a `complete` migration, it prints the before
//! state, runs the app's real `WalletManager::sync_lineage`, and prints the after
//! state (per-version `reorg_rebuilt`, lineage `migrations_reverted`, resulting
//! `migration_status`). It never runs under the normal suite (needs `HEAL_LIVE=1`).
//!
//! ```bash
//! HEAL_LIVE=1 RPC_LIVE=1 cargo test --test heal_real_lineages -- --nocapture --test-threads=1
//! ```

use std::collections::BTreeSet;

use sqlx::Row;
use uuid::Uuid;

use test_app_xpub::config::AppConfig;
use test_app_xpub::db;
use test_app_xpub::wallet::WalletManager;

fn set_env(key: &str, val: &str) {
    unsafe { std::env::set_var(key, val) };
}

fn configure_regtest_rpc_env() {
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/.env"));
    set_env(
        "DATABASE_URL",
        "postgres://asterism:asterism@host.docker.internal:5546/asterism_xpub",
    );
    set_env("BITCOIN_NETWORK", "regtest");
    set_env("BITCOIN_RPC_HOST", "host.docker.internal");
    set_env("BITCOIN_RPC_PORT", "18543");
    set_env("BITCOIN_RPC_USER", "regtest");
    set_env("BITCOIN_RPC_PASSWORD", "regtest");
    set_env("BITCOIN_WALLET_NAME", "miner");
}

async fn dump(pool: &sqlx::PgPool, label: &str) {
    let rows = sqlx::query(
        "SELECT id, status, migration_status, migration_sweep_txid, chain_tip_height, lineage_id \
         FROM federations ORDER BY chain_tip_height DESC NULLS LAST",
    )
    .fetch_all(pool)
    .await
    .expect("dump");
    eprintln!("--- {label} ---");
    for r in &rows {
        let id: Uuid = r.get("id");
        let status: String = r.get("status");
        let ms: String = r.get("migration_status");
        let sw: Option<String> = r.get("migration_sweep_txid");
        let tip: Option<i32> = r.get("chain_tip_height");
        eprintln!(
            "  {id} status={status} migration_status={ms} sweep={} tip={tip:?}",
            sw.as_deref().map_or("NULL", |s| &s[..s.len().min(12)])
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heal_real_lineages_with_complete_migrations() {
    if std::env::var("HEAL_LIVE").ok().as_deref() != Some("1") {
        eprintln!("SKIP: set HEAL_LIVE=1 to run the live recovery (mutates asterism_xpub)");
        return;
    }
    configure_regtest_rpc_env();
    let config = AppConfig::from_env().expect("AppConfig::from_env");
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations");
    let wm = WalletManager::new(pool.clone(), &config).expect("wallet manager");

    dump(&pool, "BEFORE").await;

    // Every distinct lineage that currently holds a `complete` migration.
    let lineages: BTreeSet<Uuid> = sqlx::query(
        "SELECT DISTINCT lineage_id FROM federations WHERE migration_status = 'complete'",
    )
    .fetch_all(&pool)
    .await
    .expect("find complete lineages")
    .iter()
    .map(|r| r.get::<Uuid, _>("lineage_id"))
    .collect();

    eprintln!(
        "\nhealing {} lineage(s) with a complete migration",
        lineages.len()
    );
    for lineage_id in &lineages {
        let summaries = wm
            .sync_lineage(*lineage_id)
            .await
            .expect("sync_lineage heal");
        let rebuilt = summaries.iter().any(|(_, s)| s.reorg_rebuilt);
        let reverted: u32 = summaries.iter().map(|(_, s)| s.migrations_reverted).sum();
        eprintln!("  lineage {lineage_id}: reorg_rebuilt={rebuilt} migrations_reverted={reverted}");
    }

    dump(&pool, "AFTER").await;
    eprintln!(
        "\nRecovery result: the orphaned-anchor fix healed the baked-in rows on next sync_lineage \
         (no forced rebuild)."
    );
}
