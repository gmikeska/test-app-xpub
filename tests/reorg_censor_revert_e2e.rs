//! Live e2e — **confirmation-loss reconciliation** (migration 0008), the REVERT
//! half of the censorship-reorg A/B. Proves that a migration whose sweep was
//! confirmed-then-**demoted-to-unconfirmed** by a *funds-preserving censorship
//! reorg* is reverted `complete -> pending`, while the funding tx stays confirmed
//! (the money is preserved in v0, re-migratable).
//!
//! ## Why this is the case the old predicate could not see (RED vs GREEN)
//! The 0007 reconcile predicate was *presence*-based: revert iff the recorded
//! sweep is absent from the wallet graph entirely (confirmed OR
//! unconfirmed-in-mempool both count as "present"). A **censorship** reorg —
//! invalidate the sweep's block, then extend a strictly-longer chain of EMPTY
//! blocks so the sweep is never re-mined — strips the sweep of its confirmation
//! but leaves it floating unconfirmed in the mempool. Under the presence
//! predicate that sweep still reads "present", so the migration stays stuck
//! `complete` — the live half-failure this closes (**RED**).
//!
//! The 0008 predicate is *confirmation*-loss based: latch the height at which the
//! sweep was first seen confirmed, then revert iff (height latched) AND (the
//! sweep is no longer canonically confirmed). A confirmed-then-unconfirmed sweep
//! is unambiguous reorg evidence, so it reverts (**GREEN**) — while a sweep that
//! merely never confirmed yet (NULL height) is never demote-reverted.
//!
//! This one run witnesses **both**: after the censorship reorg the sweep S is
//! still *present* in the wallet graph (the RED input the presence predicate
//! would read as "no revert") yet has lost its confirmation (the GREEN input that
//! drives the actual revert), and the funding tx D stays confirmed.
//!
//! ## Geometry (no federation signing — miner-funded, descriptor-only wallet)
//! Mirrors the sibling e2es' shortcut of using miner-signed txs the federation
//! *receives* as stand-ins (the reconcile predicate is signing-agnostic):
//!   * D = miner -> v0 address A, confirmed at h_D           (the preserved funds)
//!   * S = miner -> v0 address B, confirmed at h_S > h_D     (the recorded sweep)
//!
//! h_D < h_S is the censorship invariant (invalidating S's block must not drop D).
//! Both are tracked by v0's wallet, so the wallet sees S confirmed (latch) then
//! unconfirmed (revert), and D confirmed throughout.
//!
//! ## Harness (gv-regtest)
//!   * regtest bitcoind — JSON-RPC `host.docker.internal:18543` (`regtest`/`regtest`)
//!   * Postgres — `host.docker.internal:5546/asterism_xpub`
//!
//! No hardware is touched — the federation is descriptor-only. Opt-in gate
//! (skips cleanly otherwise): `RPC_LIVE=1`. Run from `test-app-xpub/`:
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_censor_revert_e2e -- --nocapture --test-threads=1
//! ```

// Test-local lints: regtest sat/BTC conversions do lossy int/float casts, the
// scenario body is a long linear script (same set the sibling e2es tolerate), and
// the doc header uses math notation (h_D, h_S, B_S) that reads better unbackticked.
#![allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::doc_markdown
)]

use std::process::Command;

use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use emvault::core::bitcoin::bip32::DerivationPath;
use emvault::core::bitcoin::{Amount, Network, Txid};
use emvault::core::{NetworkType, build_federation};
use emvault::xpub::DeviceType;
use emvault::xpub::test_utils::TestExternalSigner;

use test_app_xpub::config::AppConfig;
use test_app_xpub::db::{self, NewFederation};
use test_app_xpub::wallet::{AddressReceipt, FederationWallet, WalletManager};

const FUND_SATS: u64 = 500_000_000; // 5 BTC funding D (preserved in v0)
const SWEEP_SATS: u64 = 200_000_000; // 2 BTC sweep S (recorded; censored)
const CONFIRM: u32 = 3; // blocks the censorship branch buries the old tip by

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
        json!({"jsonrpc": "1.0", "id": "censor-revert-e2e", "method": method, "params": params});
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

/// Mine `n` blocks to the miner wallet (includes mempool txs).
fn mine(n: u32) -> u64 {
    let addr = rpc("getnewaddress", json!([]), Some(&miner_path()));
    let addr = addr.as_str().expect("getnewaddress");
    rpc("generatetoaddress", json!([n, addr]), None);
    block_count()
}
/// Mine one **empty** block (coinbase only, mempool-excluding) — the censorship
/// primitive: extends the chain without ever re-mining the demoted sweep S.
fn mine_empty() -> u64 {
    let addr = rpc("getnewaddress", json!([]), Some(&miner_path()));
    let addr = addr.as_str().expect("getnewaddress");
    rpc("generateblock", json!([addr, []]), None);
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
fn confirmations(txid: &str) -> i64 {
    let v = rpc("getrawtransaction", json!([txid, true]), None);
    v.get("confirmations").and_then(Value::as_i64).unwrap_or(0)
}
fn in_mempool(txid: &str) -> bool {
    let v = rpc("getrawmempool", json!([]), None);
    v.as_array()
        .is_some_and(|a| a.iter().any(|t| t.as_str() == Some(txid)))
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
    let label = format!("censor-revert-e2e-{}", Uuid::new_v4());
    let spec = NewFederation {
        label: &label,
        threshold: 2,
        total_signers: 3,
        network: "regtest",
        descriptor: &built.descriptor_string,
        snapshot_json: &built.snapshot_json,
    };
    db::insert_federation_with_members(pool, &spec, &[])
        .await
        .expect("insert federation v0")
}

/// Read (`migration_status`, `migration_sweep_txid`, `migration_sweep_confirmed_height`).
async fn read_status(
    pool: &sqlx::PgPool,
    version_id: Uuid,
) -> (String, Option<String>, Option<i32>) {
    let row = sqlx::query(
        "SELECT migration_status, migration_sweep_txid, migration_sweep_confirmed_height \
         FROM federations WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(pool)
    .await
    .expect("read version status");
    (
        row.get("migration_status"),
        row.get("migration_sweep_txid"),
        row.get("migration_sweep_confirmed_height"),
    )
}

/// This run's wallet-ground-truth receipt for `txid` at `addr`. Filters the
/// address's receipts to the specific txid, so leftover UTXOs from prior runs
/// at the same (shared BIP-39 test-vector) descriptor index — which the reorg's
/// from-scratch rescan legitimately re-discovers — are ignored. Asserting on the
/// exact D/S txids is a strictly more precise proof of "D preserved, S demoted"
/// than an aggregate wallet balance, which those leftovers pollute.
async fn receipt(wallet: &FederationWallet, addr: &str, txid: &str) -> AddressReceipt {
    let parsed = wallet.parse_address(addr).expect("parse v0 address");
    let want: Txid = txid.parse().expect("parse txid");
    let activity = wallet
        .address_history(&parsed)
        .await
        .expect("address history");
    activity
        .receipts
        .into_iter()
        .find(|r| r.txid == want)
        .unwrap_or_else(|| panic!("no wallet receipt for txid {txid} at {addr}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn censorship_reorg_reverts_on_confirmation_loss() {
    if skip() {
        return;
    }
    configure_regtest_rpc_env();

    let config = AppConfig::from_env().expect("regtest AppConfig::from_env");
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations (incl. 0008)");
    let wm = WalletManager::new(pool.clone(), &config).expect("wallet manager");

    let fed_id = provision_federation(&pool, &config).await;
    let lineage_id = fed_id; // brand-new federation == v0 of a lineage keyed by its id
    let wallet = wm
        .load_or_init(fed_id)
        .await
        .expect("load FederationWallet");

    // --- Phase 1: fund v0 with D (addr A) and the sweep S (addr B) ----------
    // Two distinct v0 addresses so the wallet tracks both; D in an earlier block
    // than S (the censorship invariant h_D < h_S).
    let addrs = wallet.reveal_addresses(2).await.expect("reveal v0 addrs");
    let addr_a = addrs[0].address.clone();
    let addr_b = addrs[1].address.clone();
    eprintln!("v0 addr A (funding D)={addr_a}\nv0 addr B (sweep  S)={addr_b}");

    let d_txid = rpc(
        "sendtoaddress",
        json!([addr_a, btc(FUND_SATS)]),
        Some(&miner_path()),
    )
    .as_str()
    .expect("D txid")
    .to_string();
    let h_d = mine(1); // D confirmed here
    let s_txid = rpc(
        "sendtoaddress",
        json!([addr_b, btc(SWEEP_SATS)]),
        Some(&miner_path()),
    )
    .as_str()
    .expect("S txid")
    .to_string();
    let h_s = mine(1); // S confirmed here (strictly above D)
    let b_s = block_hash(h_s);
    let h_pre = mine(2); // bury S below the tip
    assert!(h_s > h_d, "censorship invariant: S must be above D");
    eprintln!("D={d_txid} @h_d={h_d}; S={s_txid} @h_s={h_s} (B_S={b_s}); pre-reorg tip={h_pre}");

    // --- Phase 2: sync — wallet sees D and S both confirmed -----------------
    let s1 = wallet.sync().await.expect("sync #1");
    let bal1 = wallet.balance().await;
    eprintln!(
        "sync #1: tip={} confirmed={} total={}",
        s1.tip_height,
        bal1.confirmed,
        bal1.total()
    );
    // Per-txid wallet ground-truth (immune to leftover UTXOs at the shared
    // descriptor's reused indices): v0 must see BOTH D and S confirmed pre-reorg.
    let d1 = receipt(&wallet, &addr_a, &d_txid).await;
    let s1r = receipt(&wallet, &addr_b, &s_txid).await;
    assert_eq!(d1.amount, Amount::from_sat(FUND_SATS), "D amount");
    assert!(
        d1.confirmation_height.is_some(),
        "D must be confirmed before the reorg"
    );
    assert_eq!(s1r.amount, Amount::from_sat(SWEEP_SATS), "S amount");
    assert!(
        s1r.confirmation_height.is_some(),
        "S must be confirmed before the reorg (so its height can be latched)"
    );

    // --- Enact: flip v0 -> complete, recording S as the migration sweep -----
    db::set_migration_complete(&pool, fed_id, &s_txid)
        .await
        .expect("mark v0 complete + record sweep S");

    // --- Latch: a sync while S is still confirmed records its confirmation
    // height (0008's durable "was confirmed" witness). The confirmation-loss
    // predicate only reverts a sweep that WAS confirmed, so this must precede the
    // reorg (mirrors the real lifecycle: the sweep confirms, a reload latches it).
    let s_latch = wm.sync_lineage(lineage_id).await.expect("latch sync");
    assert_eq!(
        s_latch
            .iter()
            .map(|(_, s)| s.migrations_reverted)
            .sum::<u32>(),
        0,
        "a still-confirmed sweep must latch, not revert"
    );
    let (st1, tx1, h_latched) = read_status(&pool, fed_id).await;
    assert_eq!(st1, "complete");
    assert_eq!(tx1.as_deref(), Some(s_txid.as_str()));
    assert_eq!(
        h_latched,
        Some(h_s as i32),
        "S's confirmation height must be latched forward (durable witness)"
    );
    eprintln!("latched: status=complete sweep=S confirmed_height={h_latched:?}");

    // --- Phase 3: CENSORSHIP REORG — demote S, preserve D -------------------
    // invalidate B_S (S returns to mempool; D — an earlier block — untouched),
    // then extend a strictly-longer chain of EMPTY blocks so S is never re-mined.
    rpc("invalidateblock", json!([b_s]), None);
    // From (h_s - 1) we must pass h_pre by CONFIRM blocks.
    let need = (h_pre - (h_s - 1)) as u32 + CONFIRM;
    let mut tip = 0;
    for _ in 0..need {
        tip = mine_empty();
    }
    assert!(tip > h_pre, "censorship branch must be strictly longer");
    // Chain-side ground truth: D confirmed (preserved), S unconfirmed-in-mempool.
    assert!(
        confirmations(&d_txid) > 0,
        "D must stay confirmed (funds preserved)"
    );
    assert_eq!(confirmations(&s_txid), 0, "S must lose its confirmation");
    assert!(
        in_mempool(&s_txid),
        "S must float unconfirmed in the mempool"
    );
    eprintln!(
        "post-censor tip={tip}: D confirmations={} S confirmations={} S in_mempool={}",
        confirmations(&d_txid),
        confirmations(&s_txid),
        in_mempool(&s_txid)
    );

    // --- Phase 4: THE OBSERVATION — sync_lineage reconciles -----------------
    let s2 = wm.sync_lineage(lineage_id).await.expect("sync #2 (censor)");
    let reverted: u32 = s2.iter().map(|(_, s)| s.migrations_reverted).sum();
    let bal2 = wallet.balance().await;
    let pending2 = bal2.total() - bal2.confirmed;
    eprintln!(
        "sync #2: migrations_reverted={reverted} confirmed={} pending={pending2} total={}",
        bal2.confirmed,
        bal2.total()
    );

    // RED input (old presence predicate): S is STILL PRESENT in v0's wallet
    // graph, now demoted to unconfirmed-in-mempool — the exact input a presence
    // check (confirmed OR unconfirmed = "present") reads as "present, don't
    // revert". `receipt` panicking on absence proves S is still present; the
    // `is_none` confirmation height proves it lost its confirmation (GREEN input).
    let s2r = receipt(&wallet, &addr_b, &s_txid).await;
    assert_eq!(
        s2r.amount,
        Amount::from_sat(SWEEP_SATS),
        "S amount unchanged"
    );
    assert!(
        s2r.confirmation_height.is_none(),
        "S must still be PRESENT but demoted to UNCONFIRMED — the input a presence \
         predicate reads as 'present' (RED) yet the confirmation-loss predicate \
         reverts on (GREEN)"
    );
    // GREEN outcome (new confirmation-loss predicate): S lost its confirmation
    // (latched height, no longer confirmed) -> revert fires despite S present.
    assert_eq!(
        reverted, 1,
        "the sweep lost its confirmation (was latched, now unconfirmed) -> revert (GREEN)"
    );
    let (st2, tx2, h2) = read_status(&pool, fed_id).await;
    assert_eq!(st2, "pending", "v0 reverted complete -> pending");
    assert_eq!(tx2, None, "v0 sweep txid cleared to NULL");
    assert_eq!(h2, None, "v0 latched confirmation height cleared to NULL");
    // Funds preserved: D stays confirmed in v0 (re-migratable), asserted per-txid
    // so leftover UTXOs the rescan re-discovers at the shared descriptor's reused
    // indices don't mask the property.
    let d2 = receipt(&wallet, &addr_a, &d_txid).await;
    assert_eq!(d2.amount, Amount::from_sat(FUND_SATS), "D amount preserved");
    assert!(
        d2.confirmation_height.is_some(),
        "D's funds must be preserved (still confirmed) in v0 after the censorship reorg"
    );

    // --- Phase 5: idempotency ----------------------------------------------
    let s3 = wm
        .sync_lineage(lineage_id)
        .await
        .expect("sync #3 (idempotency)");
    let reverted3: u32 = s3.iter().map(|(_, s)| s.migrations_reverted).sum();
    assert_eq!(
        reverted3, 0,
        "already-pending v0 must not be reverted again"
    );
    let (st3, _, _) = read_status(&pool, fed_id).await;
    assert_eq!(st3, "pending", "v0 stays pending after re-sync");

    // --- Cleanup ------------------------------------------------------------
    let _ = sqlx::query("DELETE FROM federations WHERE lineage_id = $1")
        .bind(lineage_id)
        .execute(&pool)
        .await;
    eprintln!(
        "PASS: censorship reorg (S confirmed->unconfirmed, D preserved) reverted the \
         completed migration on confirmation loss; RED presence-predicate input \
         (S present) coexisted with GREEN revert; idempotent; cleaned up"
    );
}
