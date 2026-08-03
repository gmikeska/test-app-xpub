//! Live e2e — **WARM-wallet** app-side reorg reconciliation through
//! `test-app-xpub`'s real `WalletManager::sync_lineage` (**bitcoind-RPC** backend).
//!
//! This is the regression test for the warm/dense-checkpoint reorg miss that the
//! cold `reorg_reconciliation_e2e` never exercised. The running app re-syncs on
//! *every* page load: each sync builds a **fresh** `bdk_bitcoind_rpc::Emitter`
//! from the wallet's persisted checkpoint, so a long-lived wallet accumulates a
//! **dense** local-chain checkpoint set — including the block that later becomes
//! a reorg's fork point. When the reorg's fork point is still a checkpoint the
//! wallet holds, bdk's emitter rolls back to it and re-emits the replacement
//! blocks cleanly: `apply_block_connected_to` reconnects through the shared
//! ancestor and **never raises `CannotConnect`**. Before the `emitter_sync` fix,
//! that path returned `reorg_rebuilt: false` with the reorged-out tx left as a
//! stale-anchored phantom UTXO, so `sync_lineage`'s reconciliation never armed
//! and the completed migration stayed `complete` (the exact production symptom).
//!
//! The cold sibling reorgs immediately after a single sync, so its fork point is
//! the freshly-seeded birthday and the reorg surfaces as `CannotConnect` +
//! rebuild — masking the warm miss. This test instead **syncs after every mined
//! block** (mirroring per-page-load syncs) so the wallet holds the fork point as
//! a settled checkpoint, reproducing the absorbed reorg. With the fix,
//! `emitter_sync` detects the absorbed reorg (a previously-confirmed tx that is no
//! longer confirmed) and routes it through the same from-scratch rebuild:
//! `reorg_rebuilt: true`, the phantom is replaced out, and reconciliation reverts
//! the migration `complete -> pending`.
//!
//! ## Harness (gv-regtest)
//!   * regtest bitcoind — JSON-RPC `host.docker.internal:18543` (`regtest`/`regtest`)
//!   * Postgres — `host.docker.internal:5546/asterism_xpub`
//!
//! No hardware is touched — the federation is descriptor-only. Opt-in gate
//! (skips cleanly otherwise): `RPC_LIVE=1`. Run from `test-app-xpub/`:
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_warm_reconciliation_e2e -- --nocapture --test-threads=1
//! ```

// Same test-local lints the cold e2e tolerates: regtest sat/BTC conversions do
// lossy int/float casts and the scenario body is a long linear script.
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

/// Minimal bitcoind JSON-RPC over `curl` (mirrors the cold e2e).
fn rpc(method: &str, params: Value, wallet: Option<&str>) -> Value {
    let url = format!("{}{}", rpc_base(), wallet.unwrap_or(""));
    let body =
        json!({"jsonrpc": "1.0", "id": "warm-recon-xpub-e2e", "method": method, "params": params});
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
    let label = format!("reorg-warm-xpub-e2e-{}", Uuid::new_v4());
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warm_wallet_reorg_reverts_completed_migration() {
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

    // Federation created *now* → its wallet birthday is the current node tip. The
    // funding + fork point land above the birthday, exactly like the real fed.
    let fed_id = provision_federation(&pool, &config).await;
    let lineage_id = fed_id; // v0 of a lineage identified by its own id.
    let wallet = wm
        .load_or_init(fed_id)
        .await
        .expect("load FederationWallet");

    // Warm the wallet *forward past its birthday first*: mine a few blocks and
    // sync them so the wallet holds normally-emitted checkpoints above the lone
    // inserted birthday. This matters — the reorg's fork point must be one of
    // these normally-emitted blocks. A fork *at the birthday checkpoint* (a bare
    // inserted `BlockId`) can't be cleanly reconnected and surfaces as
    // `CannotConnect` (the cold-e2e path); a fork at a normally-emitted block is
    // *absorbed* by the emitter with no error — the production warm path this
    // test exists to cover.
    for _ in 0..3 {
        mine(1);
        let _ = wallet
            .sync()
            .await
            .expect("warm forward sync past birthday");
    }

    // --- Phase 1: fund a v0 address, then WARM it — sync after every block ----
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

    // Confirm D, then bury it several blocks deep — syncing the WARM wallet after
    // each mined block so its local chain accumulates a *dense* checkpoint set
    // (fork point included), reproducing a long-lived running wallet. This is the
    // one behaviour the cold e2e (single post-mine sync) never builds.
    let h0 = mine(1); // D's block
    let b0 = block_hash(h0);
    eprintln!("D confirmed in B0 height={h0}");
    // Sync D in, then advance ONE block and sync again — matching the real fed's
    // shallow geometry (funding at H, a later tx/block at H+1, wallet settled at
    // H+1, reorg fork at H-1 = two below the wallet tip). Each sync is a fresh
    // Emitter, as every page load is.
    let _ = wallet.sync().await.expect("warm sync: D confirmed");
    mine(1); // the block above D (the real fed had the sweep S here)
    let s_warm = wallet.sync().await.expect("warm sync at pre-reorg tip");
    let h_pre = block_count();
    let bal1 = wallet.balance().await.total();
    eprintln!(
        "pre-reorg: tip={h_pre} wallet_tip={} balance={bal1} reorg_rebuilt={}",
        s_warm.tip_height, s_warm.reorg_rebuilt
    );
    assert_eq!(
        bal1,
        Amount::from_sat(FUND_SATS),
        "v0 must see the funding UTXO before the reorg"
    );

    // --- Enact: optimistically flip v0 -> complete, recording the sweep txid ---
    db::set_migration_complete(&pool, fed_id, &d_txid)
        .await
        .expect("mark v0 complete + record sweep txid");
    let (st, tx) = read_status(&pool, fed_id).await;
    assert_eq!(st, "complete");
    assert_eq!(tx.as_deref(), Some(d_txid.as_str()));

    // --- Latch the sweep's confirmation height (migration 0008) while it is
    // still confirmed, before the reorg strips it. The confirmation-loss
    // predicate only reverts a sweep observed confirmed; the post-reorg
    // `reverted == 1` assertion below would fail if this latch did not fire.
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

    // --- Phase 2: reorg below the persisted tip, evicting D ------------------
    // Fork point = h0 - 1, which the warm wallet holds as a settled checkpoint,
    // so bdk's emitter absorbs the reorg (no CannotConnect) — the production path.
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
    let d_prime = rpc(
        "sendrawtransaction",
        // maxfeerate: 0 disables the fee-cap; D' carries a deliberately huge
        // absolute fee to RBF-evict the funding tx (else "Fee exceeds maximum").
        json!([signed["hex"].as_str().unwrap(), 0]),
        None,
    );
    eprintln!("broadcast D' (evicts D): {}", d_prime.as_str().unwrap());
    let h_post = mine((h_pre - block_count()) as u32 + 3);
    assert!(h_post > h_pre, "reorg branch must be strictly longer");
    eprintln!("post-reorg tip={h_post} (was {h_pre})");

    // --- Phase 3: THE OBSERVATION — warm lineage sync drives rebuild+reconcile-
    let s2 = wm.sync_lineage(lineage_id).await.expect("sync #2 (reorg)");
    let reorg_rebuilt = s2.iter().any(|(_, s)| s.reorg_rebuilt);
    let reverted: u32 = s2.iter().map(|(_, s)| s.migrations_reverted).sum();
    let bal2 = wallet.balance().await.total();
    eprintln!(
        "sync #2 (warm): reorg_rebuilt={reorg_rebuilt} migrations_reverted={reverted} balance={bal2}"
    );
    assert!(
        reorg_rebuilt,
        "warm sync must detect + rebuild the absorbed reorg (fork point in local chain)"
    );
    assert_eq!(
        reverted, 1,
        "the completed migration whose sweep was evicted must be reverted"
    );
    let (st2, tx2) = read_status(&pool, fed_id).await;
    assert_eq!(st2, "pending", "v0 reverted complete -> pending");
    assert_eq!(tx2, None, "v0 sweep txid cleared to NULL");
    assert_eq!(
        bal2,
        Amount::from_sat(0),
        "the §6.2 funding-double-spend also drops the deposit (phantom cleared, v0 -> 0)"
    );

    // --- Phase 4: idempotency -----------------------------------------------
    let s3 = wm
        .sync_lineage(lineage_id)
        .await
        .expect("sync #3 (idempotency)");
    let reverted3: u32 = s3.iter().map(|(_, s)| s.migrations_reverted).sum();
    eprintln!("sync #3 (warm): migrations_reverted={reverted3}");
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
        "PASS: warm xpub RPC-backend reorg (absorbed, no CannotConnect) reverted the completed migration; idempotent; cleaned up"
    );
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
