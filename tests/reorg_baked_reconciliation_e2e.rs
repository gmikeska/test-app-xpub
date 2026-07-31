//! Live e2e — **BAKED-IN** reorg reconciliation regression (bitcoind-RPC backend).
//!
//! This closes the coverage hole the warm e2e (`reorg_warm_reconciliation_e2e`)
//! still left open. That test reorgs and detects **within a single in-process
//! sync** — the reorged-out tx transitions confirmed→unconfirmed inside one
//! `emitter_sync` call, so the same-pass diff sees it. Production doesn't work
//! that way: the app **persists** after every sync and **reloads** the wallet from
//! that changeset on the next request. If a sync ever grafts the canonical chain
//! forward while *retaining* the reorged-out tx's stale anchor (a plain forward
//! **merge**, which is exactly what a build lacking the reorg-rebuild path does),
//! the damage is **frozen into the persisted changeset**. On reload the reorged-out
//! tx canonicalizes as unconfirmed immediately, so the same-pass confirmed→
//! unconfirmed diff has nothing to compare and returns `reorg_rebuilt = false`
//! forever — the migration stays `complete` and the phantom sweep output stays
//! canonical. This is the exact production miss observed against `asterism_xpub`
//! (v0 `superseded/complete`, tip 425, an orphaned anchor at the reorged-out block).
//!
//! The fix keys reorg detection on the **orphaned anchor** — a tx-graph anchor
//! whose block is absent from the wallet's own local chain — which *survives the
//! reload* and heals a wallet that already baked in the damage.
//!
//! This test **manufactures the baked-in state faithfully**: it funds + confirms
//! a v0 UTXO, reorgs it out, then replays the pre-fix behaviour by driving the
//! emitter forward and persisting a **merge** (not a rebuild) — writing the
//! orphaned-anchor changeset straight to the row. A **fresh** `WalletManager`
//! (cold cache) then reloads that row and runs `sync_lineage`; with the fix the
//! orphaned anchor drives a rebuild + revert `complete -> pending`; idempotent.
//!
//! ## Harness (gv-regtest)
//!   * regtest bitcoind — JSON-RPC `host.docker.internal:18543` (`regtest`/`regtest`)
//!   * Postgres — `host.docker.internal:5546/asterism_xpub`
//!
//! Opt-in gate (skips cleanly otherwise): `RPC_LIVE=1`. Run from `test-app-xpub/`:
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_baked_reconciliation_e2e -- --nocapture --test-threads=1
//! ```

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

use emvault::core::bdk_bitcoind_rpc::{Emitter, NO_EXPECTED_MEMPOOL_TXS};
use emvault::core::bdk_wallet::chain::Merge;
use emvault::core::bitcoin::bip32::DerivationPath;
use emvault::core::bitcoin::{Amount, Network};
use emvault::core::bitcoincore_rpc::{Auth, Client as RpcClient};
use emvault::core::chain_sync;
use emvault::core::{NetworkType, build_federation};
use emvault::xpub::DeviceType;
use emvault::xpub::test_utils::TestExternalSigner;

use test_app_xpub::config::AppConfig;
use test_app_xpub::db::{self, NewFederation};
use test_app_xpub::wallet::WalletManager;

const FUND_SATS: u64 = 500_000_000; // 5 BTC deposit
const EVICT_FEE_SATS: u64 = 1_000_000; // dwarfs the funding fee, replaces D

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

fn rpc(method: &str, params: Value, wallet: Option<&str>) -> Value {
    let url = format!("{}{}", rpc_base(), wallet.unwrap_or(""));
    let body =
        json!({"jsonrpc": "1.0", "id": "baked-recon-xpub-e2e", "method": method, "params": params});
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

fn skip() -> bool {
    if std::env::var("RPC_LIVE").ok().as_deref() != Some("1") {
        eprintln!("SKIP: set RPC_LIVE=1 (needs gv-regtest bitcoind + Postgres)");
        return true;
    }
    false
}

async fn provision_federation(pool: &sqlx::PgPool) -> Uuid {
    // Unique BIP-48 account per run (`m/48'/1'/<acct>'/2'`) → pristine,
    // never-funded addresses. The shared fixed `federation_derivation_path`
    // reuses indices across runs, so on this long-lived regtest node the
    // from-scratch rebuild legitimately re-discovers leftover UTXOs from prior
    // runs and the post-heal `balance == 0` assertion is masked. The mnemonics
    // carry no value, so a novel account is free.
    let acct = (Uuid::new_v4().as_u128() as u32) & 0x7fff_ffff;
    let path: DerivationPath = format!("m/48'/1'/{acct}'/2'")
        .parse()
        .expect("unique federation derivation path");
    let signers = MNEMONICS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            TestExternalSigner::from_mnemonic(
                m,
                "",
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
    let label = format!("reorg-baked-xpub-e2e-{}", Uuid::new_v4());
    let spec = NewFederation {
        label: &label,
        threshold: 2,
        total_signers: 3,
        network: "regtest",
        descriptor: &built.descriptor_string,
        snapshot_json: &built.snapshot_json,
        master_blinding_key: None,
    };
    db::insert_federation_with_members(pool, &spec, &[])
        .await
        .expect("insert federation v0")
}

/// Replay the **pre-fix** persist: load the row's current changeset, drive the
/// emitter forward against the (already reorged) node, apply mempool, and persist
/// a **merge** — deliberately *not* a rebuild. The reorged-out tx's stale anchor
/// is retained, so the written changeset carries an **orphaned anchor** with a
/// fully-canonical local chain: the exact production baked-in state.
async fn bake_in_graft(pool: &sqlx::PgPool, rpc_client: &RpcClient, fed_id: Uuid) {
    let row = db::find_federation_by_id(pool, fed_id)
        .await
        .expect("find row")
        .expect("row exists");
    let loaded =
        chain_sync::init_or_load_wallet(Network::Regtest, row.descriptor, row.bdk_changeset)
            .expect("load persisted (pre-reorg) wallet");
    let mut wallet = loaded.wallet;
    let mut agg = loaded.changeset;

    let cp = wallet.latest_checkpoint();
    let start = cp.height();
    let mut emitter = Emitter::new(rpc_client, cp, start, NO_EXPECTED_MEMPOOL_TXS);
    while let Some(ev) = emitter.next_block().expect("emit block") {
        let height = ev.block_height();
        let connected_to = ev.connected_to();
        // Absorb the reorg the way the pre-fix path did: apply forward. bdk updates
        // the local chain to canonical yet retains the reorged-out tx's anchor.
        wallet
            .apply_block_connected_to(&ev.block, height, connected_to)
            .expect("apply forward (absorb reorg)");
    }
    let mempool = emitter.mempool().expect("mempool");
    wallet.apply_unconfirmed_txs(mempool.update);

    if let Some(delta) = wallet.take_staged() {
        agg.merge(delta); // MERGE, not replace — this is the bug being reproduced.
    }
    let json = serde_json::to_value(&agg).expect("encode grafted changeset");
    let tip = wallet.latest_checkpoint().height();
    db::update_federation_changeset(pool, fed_id, &json, i32::try_from(tip).unwrap_or(i32::MAX))
        .await
        .expect("persist grafted changeset");
    eprintln!(
        "baked-in graft persisted: canonical local chain @ tip {tip} + retained stale anchor"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn baked_in_reorg_reverts_completed_migration_on_reload() {
    if skip() {
        return;
    }
    configure_regtest_rpc_env();

    let config = AppConfig::from_env().expect("regtest AppConfig::from_env");
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations (incl. 0007)");
    let rpc_client = RpcClient::new(
        &config.bitcoin_rpc_url,
        Auth::UserPass(
            config.bitcoin_rpc_user.clone(),
            config.bitcoin_rpc_password.clone(),
        ),
    )
    .expect("rpc client");

    let wm = WalletManager::new(pool.clone(), &config).expect("wallet manager");
    let fed_id = provision_federation(&pool).await;
    let lineage_id = fed_id;
    let wallet = wm
        .load_or_init(fed_id)
        .await
        .expect("load FederationWallet");

    // Warm forward past the birthday so the reorg fork point is a settled,
    // normally-emitted checkpoint (as in the warm e2e).
    for _ in 0..3 {
        mine(1);
        let _ = wallet.sync().await.expect("warm forward sync");
    }

    // --- Fund v0, confirm, and settle at the pre-reorg tip -------------------
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

    let h0 = mine(1); // D's block
    let b0 = block_hash(h0);
    eprintln!("D confirmed in B0 height={h0}");
    let _ = wallet.sync().await.expect("sync: D confirmed");
    mine(1);
    let _ = wallet.sync().await.expect("sync at pre-reorg tip");
    let h_pre = block_count();
    let bal1 = wallet.balance().await.total();
    assert_eq!(
        bal1,
        Amount::from_sat(FUND_SATS),
        "v0 must see the funding UTXO before the reorg"
    );

    // Optimistically flip v0 -> complete, recording the sweep txid (here D).
    db::set_migration_complete(&pool, fed_id, &d_txid)
        .await
        .expect("mark v0 complete + record sweep txid");

    // --- Latch the sweep's confirmation height (migration 0008) while it is
    // still confirmed, before the reorg + baked-graft. The latched height lives
    // in a `federations` column, so it survives the fresh-manager reload the heal
    // performs; the confirmation-loss predicate then reverts the reloaded row on
    // finding the sweep no longer confirmed. Without this the `reverted == 1`
    // heal assertion below would fail.
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

    // --- Reorg below the persisted tip, evicting D --------------------------
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

    // --- Manufacture the baked-in state: pre-fix forward-merge persist -------
    bake_in_graft(&pool, &rpc_client, fed_id).await;
    // Sanity: the row is still `complete` and the persisted changeset carries an
    // orphaned anchor (verified structurally by the heal below).
    let (st, tx) = read_status(&pool, fed_id).await;
    assert_eq!(st, "complete", "baked-in row must still read complete");
    assert_eq!(tx.as_deref(), Some(d_txid.as_str()));

    // --- THE HEAL: a *fresh* WalletManager reloads the grafted row ----------
    // Cold cache => it reads the baked-in changeset from the DB, not any live
    // in-memory wallet. This is the production reload path.
    let wm2 = WalletManager::new(pool.clone(), &config).expect("fresh wallet manager");
    let s2 = wm2.sync_lineage(lineage_id).await.expect("heal sync");
    let reorg_rebuilt = s2.iter().any(|(_, s)| s.reorg_rebuilt);
    let reverted: u32 = s2.iter().map(|(_, s)| s.migrations_reverted).sum();
    let bal2 = wm2
        .load_or_init(fed_id)
        .await
        .expect("reload healed wallet")
        .balance()
        .await
        .total();
    eprintln!(
        "heal sync: reorg_rebuilt={reorg_rebuilt} migrations_reverted={reverted} balance={bal2}"
    );
    assert!(
        reorg_rebuilt,
        "the orphaned anchor baked into the persisted changeset must drive a rebuild on reload"
    );
    assert_eq!(reverted, 1, "the evicted-sweep migration must be reverted");
    let (st2, tx2) = read_status(&pool, fed_id).await;
    assert_eq!(st2, "pending", "v0 reverted complete -> pending");
    assert_eq!(tx2, None, "v0 sweep txid cleared to NULL");
    assert_eq!(bal2, Amount::from_sat(0), "phantom cleared, v0 -> 0");

    // --- Idempotency: healed row must not re-trigger -------------------------
    let wm3 = WalletManager::new(pool.clone(), &config).expect("third wallet manager");
    let s3 = wm3
        .sync_lineage(lineage_id)
        .await
        .expect("idempotency sync");
    let rebuilt3 = s3.iter().any(|(_, s)| s.reorg_rebuilt);
    let reverted3: u32 = s3.iter().map(|(_, s)| s.migrations_reverted).sum();
    eprintln!("idempotency sync: reorg_rebuilt={rebuilt3} migrations_reverted={reverted3}");
    assert!(
        !rebuilt3,
        "the healed (replaced) changeset carries no stale anchor, so no re-rebuild"
    );
    assert_eq!(reverted3, 0, "already-pending v0 must not revert again");
    let (st3, _) = read_status(&pool, fed_id).await;
    assert_eq!(st3, "pending", "v0 stays pending after re-sync");

    // --- Cleanup ------------------------------------------------------------
    let _ = sqlx::query("DELETE FROM federations WHERE lineage_id = $1")
        .bind(lineage_id)
        .execute(&pool)
        .await;
    eprintln!(
        "PASS: baked-in reorg (orphaned anchor persisted via merge) self-healed on reload; idempotent; cleaned up"
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
