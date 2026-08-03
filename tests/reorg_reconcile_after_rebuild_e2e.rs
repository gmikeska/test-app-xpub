//! Live e2e — **reconcile survives a rebuild consumed by an earlier sync**.
//!
//! Regression for the split-failure Greg hit on the live warm-reorg demo: the
//! `emitter_sync` warm-reorg fix fired (balance correctly went to `0`, the
//! phantom sweep was purged and the graph persist-*replaced* clean), **but the
//! migration stayed `complete`** — reconcile never reverted it. Frozen DB ground
//! truth: both versions' `bdk_changeset.tx_graph` were empty (sweep absent
//! everywhere) yet `federations.migration_status = 'complete'`.
//!
//! ## Root cause this pins
//! A reorg rebuild is **one-shot**: the custody-critical persist-*replace* on
//! `reorg_rebuilt` cleans the phantom out of the graph, so the *next* `sync()`
//! reports `reorg_rebuilt: false`. The old code gated `reconcile_reverted_migrations`
//! on an in-pass `reorg_rebuilt` flag, so a rebuild consumed by an **earlier**
//! sync — in production the throwaway header `fw.sync()` at `federation_manage`
//! (before it calls `sync_lineage`), or a prior tab reload / background sync —
//! left the migration stuck `complete` forever.
//!
//! ## What this reproduces (the coverage hole)
//! The existing `reorg_reconciliation_e2e` / `reorg_warm_reconciliation_e2e`
//! always let the rebuild land in the *same* `sync_lineage` pass that reconciles,
//! so `reorg_rebuilt` was `true` there and the gate happened to open. This test
//! instead **consumes the rebuild in a standalone `wallet.sync()` first** (the
//! header-sync analog), then runs `sync_lineage` — whose own pass therefore sees
//! `reorg_rebuilt: false`. The decisive assertion is that combination:
//!
//!   `reorg_rebuilt == false` (no rebuild in the reconciling pass)  **and**
//!   `migrations_reverted == 1` (reconcile fired anyway, on wallet ground truth)
//!
//! which is *impossible* under the old gate (`if !reorg_rebuilt { return 0 }`)
//! and is exactly the frozen stuck state healing.
//!
//! ## Harness (gv-regtest)
//!   * regtest bitcoind — JSON-RPC `host.docker.internal:18543` (`regtest`/`regtest`)
//!   * Postgres — `host.docker.internal:5546/asterism_xpub`
//!
//! No hardware is touched — the federation is descriptor-only. Opt-in gate
//! (skips cleanly otherwise): `RPC_LIVE=1`. Run from `test-app-xpub/`:
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_reconcile_after_rebuild_e2e -- --nocapture --test-threads=1
//! ```

// Test-local lints: regtest sat/BTC conversions do lossy int/float casts and the
// scenario body is a long linear script (same set the sibling e2es tolerate).
#![allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value
)]

use std::process::Command;

use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use emvault::core::bitcoin::bip32::DerivationPath;
use emvault::core::bitcoin::{Amount, Network};
use emvault::core::{NetworkType, build_federation};
use emvault::xpub::DeviceType;
use emvault::xpub::test_utils::TestExternalSigner;

use test_app_xpub::config::AppConfig;
use test_app_xpub::db::{self, NewFederation};
use test_app_xpub::wallet::WalletManager;

const FUND_SATS: u64 = 500_000_000; // 5 BTC deposit
const EVICT_FEE_SATS: u64 = 1_000_000; // dwarfs the funding fee, replaces D

// Publicly-known BIP-39 test vectors (no value); one per federation signer.
const MNEMONICS: [&str; 3] = [
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "legal winner thank year wave sausage worth useful legal winner thank yellow",
    "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
];

fn rpc_base() -> String {
    "http://host.docker.internal:18543".to_string()
}
fn rpc_auth() -> String {
    "regtest:regtest".to_string()
}
fn miner_path() -> String {
    "/wallet/miner".to_string()
}

/// Minimal bitcoind JSON-RPC over `curl` (mirrors the sibling e2es / `drive.sh`).
fn rpc(method: &str, params: Value, wallet: Option<&str>) -> Value {
    let url = format!("{}{}", rpc_base(), wallet.unwrap_or(""));
    let body =
        json!({"jsonrpc": "1.0", "id": "recon-after-rebuild", "method": method, "params": params});
    let out = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "60",
            "--user",
            &rpc_auth(),
            "--data-binary",
            &body.to_string(),
            "-H",
            "content-type: text/plain;",
            &url,
        ])
        .output()
        .expect("spawn curl");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "rpc {method}: bad JSON: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert!(
        v.get("error").is_none_or(Value::is_null),
        "rpc {method} error: {}",
        v["error"]
    );
    v["result"].clone()
}

fn mine(n: u32) -> u64 {
    let addr = rpc("getnewaddress", json!([]), Some(&miner_path()));
    let addr = addr.as_str().expect("getnewaddress");
    rpc("generatetoaddress", json!([n, addr]), None);
    block_count()
}
fn block_count() -> u64 {
    rpc("getblockcount", json!([]), None).as_u64().unwrap()
}
fn block_hash(height: u64) -> String {
    rpc("getblockhash", json!([height]), None)
        .as_str()
        .unwrap()
        .to_string()
}
fn btc(sats: u64) -> f64 {
    sats as f64 / 100_000_000.0
}

/// Set an env var (edition-2024 `set_var` is `unsafe`). Called single-threaded
/// before `AppConfig::from_env`, so no data-race concern.
fn set_env(key: &str, val: &str) {
    unsafe { std::env::set_var(key, val) };
}

/// Point `AppConfig::from_env` at the gv-regtest bitcoind on the RPC backend,
/// overriding the `.env` (signet) defaults.
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

fn skip() -> bool {
    if std::env::var("RPC_LIVE").ok().as_deref() != Some("1") {
        eprintln!("SKIP: set RPC_LIVE=1 (needs gv-regtest bitcoind + Postgres)");
        return true;
    }
    false
}

/// Build a genuine 2-of-3 `wsh(sortedmulti)` federation from the three test
/// mnemonics at the app's derivation path, and insert it as v0 of a fresh
/// lineage (no members — the reconcile path is descriptor-only).
async fn provision_federation(pool: &sqlx::PgPool, config: &AppConfig) -> Uuid {
    let path: DerivationPath = config
        .federation_derivation_path
        .parse()
        .expect("federation derivation path");
    // Per-run-unique BIP-39 passphrase => unique xpubs => unique descriptor =>
    // pristine addresses, so the live test is idempotent and never picks up
    // funds a prior run left at a shared deterministic address.
    let run_pass = Uuid::new_v4().to_string();
    let signers = MNEMONICS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            TestExternalSigner::from_mnemonic(
                m,
                &run_pass,
                &path,
                Network::Regtest,
                DeviceType::Trezor,
                Some(format!("signer-{}", i + 1)),
            )
            .expect("build test signer")
            .external_signer()
            .clone()
        })
        .collect::<Vec<_>>();

    let built = build_federation(signers, 2, NetworkType::Bitcoin(Network::Regtest))
        .expect("build 2-of-3 federation");
    let label = format!("reorg-after-rebuild-e2e-{}", Uuid::new_v4());
    let spec = NewFederation {
        label: &label,
        threshold: 2,
        total_signers: 3,
        network: "regtest",
        descriptor: &built.descriptor_string,
        elements_descriptor: None,
        snapshot_json: &built.snapshot_json,
        master_blinding_key: None,
    };
    db::insert_federation_with_members(pool, &spec, &[])
        .await
        .expect("insert federation v0")
}

async fn read_status(pool: &sqlx::PgPool, version_id: Uuid) -> (String, Option<String>) {
    let row =
        sqlx::query("SELECT migration_status, migration_sweep_txid FROM federations WHERE id = $1")
            .bind(version_id)
            .fetch_one(pool)
            .await
            .expect("read version status");
    (row.get("migration_status"), row.get("migration_sweep_txid"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_fires_after_rebuild_consumed_by_earlier_sync() {
    if skip() {
        return;
    }
    configure_regtest_rpc_env();

    let config = AppConfig::from_env().expect("regtest AppConfig::from_env");
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations (incl. 0007)");
    let wm = WalletManager::new(pool.clone(), &config).expect("wallet manager");

    let fed_id = provision_federation(&pool, &config).await;
    let lineage_id = fed_id; // brand-new federation == v0 of a lineage keyed by its id
    let wallet = wm
        .load_or_init(fed_id)
        .await
        .expect("load FederationWallet");

    // --- Phase 1: fund a v0 address, confirm, bury below the tip -----------
    let addr = wallet.reveal_addresses(1).await.expect("reveal v0 addr")[0]
        .address
        .clone();
    eprintln!("funding v0 address: {addr}");
    let d_txid = rpc(
        "sendtoaddress",
        json!([addr, btc(FUND_SATS)]),
        Some(&miner_path()),
    );
    let d_txid = d_txid.as_str().expect("sendtoaddress txid").to_string();
    let d = rpc("getrawtransaction", json!([d_txid, true]), None);
    let u_txid = d["vin"][0]["txid"].as_str().unwrap().to_string();
    let u_vout = d["vin"][0]["vout"].as_u64().unwrap();
    let u = rpc("getrawtransaction", json!([u_txid, true]), None);
    let u_value_btc = u["vout"][u_vout as usize]["value"].as_f64().unwrap();
    eprintln!("D={d_txid} U={u_txid}:{u_vout} ({u_value_btc} BTC)");

    let h0 = mine(1);
    let b0 = block_hash(h0);
    let h_pre = mine(2);
    eprintln!("D confirmed in B0 height={h0}; pre-reorg tip={h_pre}");

    let s1 = wallet.sync().await.expect("sync #1");
    let bal1 = wallet.balance().await.total();
    eprintln!("sync #1: tip={} balance={}", s1.tip_height, bal1);
    assert_eq!(
        bal1,
        Amount::from_sat(FUND_SATS),
        "v0 must see the funding UTXO before the reorg"
    );

    // --- Enact: optimistically flip v0 -> complete, recording the sweep txid.
    // (As in the sibling e2es the recorded "sweep" is the confirmed funding tx
    // the wallet holds, evicted via the proven §6.2 funding-double-spend.)
    db::set_migration_complete(&pool, fed_id, &d_txid)
        .await
        .expect("mark v0 complete + record sweep txid");
    let (st, tx) = read_status(&pool, fed_id).await;
    assert_eq!(st, "complete");
    assert_eq!(tx.as_deref(), Some(d_txid.as_str()));

    // --- Latch the sweep's confirmation height (migration 0008) while it is
    // still confirmed, before the reorg evicts it. The confirmation-loss
    // predicate only reverts a sweep observed confirmed; the `reverted == 1`
    // assertion in Phase 4 would fail if this latch did not fire.
    let s_latch = wm
        .sync_lineage(lineage_id)
        .await
        .expect("latch sync (pre-reorg)");
    assert_eq!(
        s_latch
            .iter()
            .map(|(_, s)| s.migrations_reverted)
            .sum::<u32>(),
        0,
        "a still-confirmed sweep must latch, not revert"
    );

    // --- Phase 2: reorg below the persisted tip, evicting D ----------------
    rpc("invalidateblock", json!([b0]), None);
    let dest = rpc("getnewaddress", json!([]), Some(&miner_path()));
    let out_sats = (u_value_btc * 100_000_000.0).round() as u64 - EVICT_FEE_SATS;
    let mut outputs = serde_json::Map::new();
    outputs.insert(dest.as_str().unwrap().to_string(), json!(btc(out_sats)));
    let raw = rpc(
        "createrawtransaction",
        json!([[{"txid": u_txid, "vout": u_vout}], Value::Object(outputs)]),
        None,
    );
    let signed = rpc(
        "signrawtransactionwithwallet",
        json!([raw.as_str().unwrap()]),
        Some(&miner_path()),
    );
    assert!(
        signed["complete"].as_bool().unwrap_or(false),
        "miner must sign the double-spend of U: {signed}"
    );
    // maxfeerate: 0 disables sendrawtransaction's fee-cap (default 0.10 BTC/kvB).
    // D' carries a deliberately huge absolute fee (EVICT_FEE_SATS) so it RBF-evicts
    // the funding tx; the node would otherwise reject it as "Fee exceeds maximum".
    let d_prime = rpc(
        "sendrawtransaction",
        json!([signed["hex"].as_str().unwrap(), 0]),
        None,
    );
    eprintln!("broadcast D' (evicts D): {}", d_prime.as_str().unwrap());
    let h_post = mine((h_pre - block_count()) as u32 + 3);
    assert!(h_post > h_pre, "reorg branch must be strictly longer");
    eprintln!("post-reorg tip={h_post} (was {h_pre})");

    // --- Phase 3: CONSUME the one-shot rebuild in a standalone sync ---------
    // This is the production trigger: `federation_manage` runs a throwaway
    // header `fw.sync()` on the page version *before* it calls `sync_lineage`.
    // That sync detects the warm reorg, rebuilds from scratch, persist-REPLACES
    // the graph clean (balance -> 0) and returns `reorg_rebuilt: true` — which
    // the handler discards. The rebuild is now spent.
    let s_header = wallet
        .sync()
        .await
        .expect("header-analog sync (consumes rebuild)");
    let bal_hdr = wallet.balance().await.total();
    eprintln!(
        "header-analog sync: reorg_rebuilt={} balance={bal_hdr}",
        s_header.reorg_rebuilt
    );
    assert!(
        s_header.reorg_rebuilt,
        "the standalone sync must be the one that detects+rebuilds the warm reorg"
    );
    assert_eq!(
        bal_hdr,
        Amount::from_sat(0),
        "rebuild must purge the phantom (balance -> 0) — the visible half that DID work live"
    );
    // Migration is still `complete` at this point — nothing has reconciled yet.
    let (st_mid, _) = read_status(&pool, fed_id).await;
    assert_eq!(st_mid, "complete", "no reconcile has run yet");

    // --- Phase 4: THE OBSERVATION — sync_lineage AFTER the rebuild is spent -
    // Its own sync pass sees a clean wallet, so `reorg_rebuilt` is FALSE here.
    // Old gated code: `any_reorg == false` -> reconcile early-returns -> STUCK.
    // Fixed code: reconcile keys on wallet ground truth -> sweep absent -> revert.
    let s2 = wm
        .sync_lineage(lineage_id)
        .await
        .expect("sync_lineage (reconcile)");
    let reorg_rebuilt = s2.iter().any(|(_, s)| s.reorg_rebuilt);
    let reverted: u32 = s2.iter().map(|(_, s)| s.migrations_reverted).sum();
    let bal2 = wallet.balance().await.total();
    eprintln!(
        "sync_lineage: reorg_rebuilt={reorg_rebuilt} migrations_reverted={reverted} balance={bal2}"
    );

    // The decisive A/B assertion — impossible under the old gate:
    assert!(
        !reorg_rebuilt,
        "the reconciling pass must see NO rebuild (it was consumed in Phase 3) — \
         this is exactly the frozen stuck geometry"
    );
    assert_eq!(
        reverted, 1,
        "reconcile must STILL revert the completed-but-evicted migration even though \
         no rebuild happened in this pass (the fix)"
    );
    let (st2, tx2) = read_status(&pool, fed_id).await;
    assert_eq!(st2, "pending", "v0 reverted complete -> pending");
    assert_eq!(tx2, None, "v0 sweep txid cleared to NULL");
    assert_eq!(
        bal2,
        Amount::from_sat(0),
        "balance stays 0 (sweep genuinely gone)"
    );

    // --- Phase 5: idempotency ----------------------------------------------
    let s3 = wm
        .sync_lineage(lineage_id)
        .await
        .expect("sync_lineage (idempotency)");
    let reverted3: u32 = s3.iter().map(|(_, s)| s.migrations_reverted).sum();
    eprintln!("re-sync: migrations_reverted={reverted3}");
    assert_eq!(
        reverted3, 0,
        "already-pending v0 must not be reverted again"
    );
    let (st3, _) = read_status(&pool, fed_id).await;
    assert_eq!(st3, "pending", "v0 stays pending after re-sync");

    // --- Cleanup ------------------------------------------------------------
    let _ = sqlx::query("DELETE FROM federations WHERE lineage_id = $1")
        .bind(lineage_id)
        .execute(&pool)
        .await;
    eprintln!(
        "PASS: reconcile fired after the rebuild was consumed by an earlier sync \
         (reorg_rebuilt=false, migrations_reverted=1); idempotent; cleaned up"
    );
}
