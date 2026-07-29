//! Live e2e — **funds-preserving migration re-completion** (migration 0009), the
//! RE-SWEEP half of the censorship-reorg story and the FULL loop end-to-end.
//!
//! Proves the whole cycle with a *real* 2-of-3 signed sweep (not a miner-funded
//! stand-in): fund `D` → sign+broadcast the migration sweep `S` (v0 → v1) →
//! confirm+latch `complete` → **censorship reorg** (strip `S`'s confirmation,
//! keep `D` confirmed) → confirmation-loss revert `complete -> pending` with the
//! funds preserved in `D` → **re-sweep** `S'` that force-respends the stranded
//! `D` and RBF-displaces `S` → broadcast+confirm → re-mark `complete` (funds now
//! in v1).
//!
//! ## The A/B (RED vs GREEN) — why re-completion needs its own builder
//! After the censorship reorg the old sweep `S` floats unconfirmed in the
//! mempool still spending `D`, so BDK marks `D` spent-by-mempool. Therefore:
//!   * **RED** — the ordinary migration drain
//!     ([`FederationWallet::build_migration_tx`]) selects nothing (no spendable
//!     UTXO) and errors. This is the exact wall a naive "just re-run the
//!     migration" hits, reproduced live.
//!   * **GREEN** — [`FederationWallet::build_migration_resweep`] transiently
//!     evicts `S` to free `D`, drains it forward at a higher fee signalling
//!     BIP-125 RBF, and the broadcast `S'` *displaces* `S` in the mempool.
//!
//! One run witnesses both: at the reverted point the drain fails while the
//! re-sweep succeeds, `S` leaves the mempool and `S'` enters, and the migration
//! ends `complete` again with the funds in v1.
//!
//! ## Faithful lifecycle (real signing, real lineage)
//!   * v0 = 2-of-3 `wsh(sortedmulti)`; v1 = 2-of-2 successor (distinct
//!     descriptor). `create_pending_migration` + `enact_version_transition` mint
//!     and flip the lineage exactly as the migration handler does.
//!   * `S` and `S'` are signed in-test by the federation's own keys
//!     (`TestExternalSigner::sign_for_test`) — 2 of the 3 mnemonic signers.
//!   * The broadcast re-mark is exercised through the real db helpers
//!     (`insert_resweep_proposal` / `resweep_recompletion_for_proposal` /
//!     `set_migration_complete`) that `proposals::broadcast` calls.
//!
//! ## Harness (gv-regtest) — same as the sibling reorg e2es
//!   * regtest bitcoind — JSON-RPC `host.docker.internal:18543` (`regtest`/`regtest`)
//!   * Postgres — `host.docker.internal:5546/asterism_xpub`
//!
//! No hardware is touched. Opt-in gate (skips cleanly otherwise): `RPC_LIVE=1`.
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_resweep_recompletion_e2e -- --nocapture --test-threads=1
//! ```

// Test-local lints: regtest sat/BTC conversions do lossy int/float casts, the
// scenario body is a long linear script, and the doc header uses math notation
// (D, S, S') that reads better unbackticked.
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
use std::str::FromStr;

use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use emvault::core::bitcoin::bip32::DerivationPath;
use emvault::core::bitcoin::{Address, Network, Psbt};
use emvault::core::{NetworkType, build_federation};
use emvault::xpub::DeviceType;
use emvault::xpub::test_utils::TestExternalSigner;

use test_app_xpub::config::AppConfig;
use test_app_xpub::db::{self, NewFederation};
use test_app_xpub::wallet::{FederationWallet, WalletManager, first_external_address};

const FUND_SATS: u64 = 500_000_000; // 5 BTC funding D (preserved in v0)
const FEE_S: u64 = 2; // sat/vB for the original sweep S
const FEE_SP: u64 = 20; // sat/vB for the re-sweep S' — strictly above S so it RBF-displaces
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
    let body = json!({"jsonrpc": "1.0", "id": "resweep-e2e", "method": method, "params": params});
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

/// Build the three test signers at a **unique** BIP-48 account path
/// (`m/48'/1'/<acct>'/2'`). A per-run account index gives this run a pristine
/// descriptor — fresh, never-funded addresses — so the RED half (the ordinary
/// drain must find *nothing* spendable after the reorg locks D) is not masked by
/// leftover UTXOs the shared fixed-path test-vector descriptor accumulates
/// across runs. The mnemonics carry no value, so a novel account is free.
fn build_signers(acct: u32) -> Vec<TestExternalSigner> {
    let path: DerivationPath = format!("m/48'/1'/{acct}'/2'")
        .parse()
        .expect("unique federation derivation path");
    MNEMONICS
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
        })
        .collect()
}

/// Sign `psbt_b64` with signers 0 and 1 (a 2-of-N quorum), finalize via the
/// wallet, and return the finalized `(tx_hex, txid)`.
async fn sign_2of(
    wallet: &FederationWallet,
    signers: &[TestExternalSigner],
    psbt_b64: &str,
) -> (String, String) {
    let base = Psbt::from_str(psbt_b64).expect("parse psbt");
    // `sign_for_test` preserves existing partial_sigs, so chaining accumulates a
    // 2-of-3 quorum without an explicit combine step.
    let p0 = signers[0].sign_for_test(&base).expect("signer 0");
    let p01 = signers[1].sign_for_test(&p0).expect("signer 1");
    let fin = wallet
        .finalize_and_extract(&p01.to_string())
        .await
        .expect("finalize 2-of-N");
    (fin.tx_hex, fin.txid.to_string())
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn censorship_reorg_resweep_recompletes_migration() {
    if skip() {
        return;
    }
    configure_regtest_rpc_env();

    let config = AppConfig::from_env().expect("regtest AppConfig::from_env");
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations (incl. 0009)");
    let wm = WalletManager::new(pool.clone(), &config).expect("wallet manager");

    // Unique BIP-48 account per run → pristine, never-funded addresses.
    let acct = (Uuid::new_v4().as_u128() as u32) & 0x7fff_ffff;
    let signers = build_signers(acct);
    let ext: Vec<_> = signers
        .iter()
        .map(|s| s.external_signer().clone())
        .collect();

    // --- Provision the lineage: v0 (2-of-3) + enacted successor v1 (2-of-2) ---
    let v0_built = build_federation(ext.clone(), 2, NetworkType::Bitcoin(Network::Regtest))
        .expect("build v0 2-of-3");
    let label = format!("resweep-e2e-{}", Uuid::new_v4());
    let v0_id = db::insert_federation_with_members(
        &pool,
        &NewFederation {
            label: &label,
            threshold: 2,
            total_signers: 3,
            network: "regtest",
            descriptor: &v0_built.descriptor_string,
            snapshot_json: &v0_built.snapshot_json,
        },
        &[],
    )
    .await
    .expect("insert federation v0");
    let lineage_id = v0_id;

    // A proposer user (federation_migrations.proposed_by is a NOT NULL FK).
    let email = format!("resweep-{}@example.test", Uuid::new_v4());
    db::upsert_user_if_absent(&pool, &email, "x")
        .await
        .expect("insert user");
    let user_id = db::find_user_by_email(&pool, &email)
        .await
        .expect("find user")
        .expect("user row")
        .id;

    // v1 = 2-of-2 successor (signers 0,1) — a distinct descriptor so its address
    // differs from v0's. Minted pending, then enacted (v0 -> superseded). Inserted
    // directly (rather than via `create_pending_migration`, which derives
    // total_signers from the member roster) so this descriptor-only harness
    // satisfies the `total_signers >= threshold` check without member rows.
    let v1_built = build_federation(
        vec![ext[0].clone(), ext[1].clone()],
        2,
        NetworkType::Bitcoin(Network::Regtest),
    )
    .expect("build v1 2-of-2");
    let v1_id: Uuid = sqlx::query_scalar(
        "INSERT INTO federations \
            (label, threshold, total_signers, network, descriptor, snapshot_json, \
             lineage_id, version_index, status, predecessor_id) \
         VALUES ($1, 2, 2, 'regtest', $2, $3, $4, 1, 'pending', $5) RETURNING id",
    )
    .bind(&label)
    .bind(&v1_built.descriptor_string)
    .bind(&v1_built.snapshot_json)
    .bind(lineage_id)
    .bind(v0_id)
    .fetch_one(&pool)
    .await
    .expect("insert pending v1");
    let migration_id: Uuid = sqlx::query_scalar(
        "INSERT INTO federation_migrations \
            (lineage_id, base_version_id, target_version_id, proposed_by, next_threshold, status) \
         VALUES ($1, $2, $3, $4, 2, 'proposed') RETURNING id",
    )
    .bind(lineage_id)
    .bind(v0_id)
    .bind(v1_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("insert migration record");
    db::enact_version_transition(&pool, v1_id, v0_id, migration_id)
        .await
        .expect("enact v0 -> v1");
    let v1_dest: Address = first_external_address(Network::Regtest, &v1_built.descriptor_string)
        .expect("v1 destination address");
    eprintln!("v0={v0_id} (superseded) v1={v1_id} (active) dest={v1_dest}");

    let v0 = wm.load_or_init(v0_id).await.expect("load v0");

    // --- Fund D -> v0, and build+sign+broadcast the real migration sweep S ----
    let addr = &v0.reveal_addresses(1).await.expect("reveal v0 addr")[0].address;
    let d_txid = rpc(
        "sendtoaddress",
        json!([addr, btc(FUND_SATS)]),
        Some(&miner_path()),
    )
    .as_str()
    .expect("D txid")
    .to_string();
    let h_d = mine(1);
    v0.sync().await.expect("sync sees D");

    let built_s = v0
        .build_migration_tx(&v1_dest, FEE_S)
        .await
        .expect("build sweep S");
    let (s_hex, s_txid) = sign_2of(&v0, &signers, &built_s.psbt_b64).await;
    let s_broadcast = v0.broadcast_raw(&s_hex).await.expect("broadcast S");
    assert_eq!(
        s_broadcast.to_string(),
        s_txid,
        "broadcast txid == finalized"
    );
    let h_s = mine(1); // S confirmed
    let b_s = block_hash(h_s);
    let h_pre = mine(2); // bury S below the tip
    assert!(h_s > h_d, "censorship invariant: S above D");
    eprintln!("D={d_txid}@{h_d}  S={s_txid}@{h_s} (B_S={b_s})  pre-reorg tip={h_pre}");

    // --- Enact complete + latch S's confirmation height -----------------------
    db::set_migration_complete(&pool, v0_id, &s_txid)
        .await
        .expect("mark v0 complete");
    let latch = wm.sync_lineage(lineage_id).await.expect("latch sync");
    assert_eq!(
        latch
            .iter()
            .map(|(_, s)| s.migrations_reverted)
            .sum::<u32>(),
        0,
        "a still-confirmed sweep latches, never reverts"
    );
    let (st1, tx1, h1) = read_status(&pool, v0_id).await;
    assert_eq!(st1, "complete");
    assert_eq!(tx1.as_deref(), Some(s_txid.as_str()));
    assert_eq!(h1, Some(h_s as i32), "S's confirmation height latched");
    // Funds moved to v1: S confirmed.
    assert!(confirmations(&s_txid) > 0, "S confirmed pre-reorg");
    eprintln!("latched: complete sweep=S height={h1:?}  (funds in v1)");

    // --- CENSORSHIP REORG: demote S, preserve D -------------------------------
    rpc("invalidateblock", json!([b_s]), None);
    let need = (h_pre - (h_s - 1)) as u32 + CONFIRM;
    let mut tip = 0;
    for _ in 0..need {
        tip = mine_empty();
    }
    assert!(tip > h_pre, "censorship branch strictly longer");
    assert!(confirmations(&d_txid) > 0, "D preserved (still confirmed)");
    assert_eq!(confirmations(&s_txid), 0, "S lost its confirmation");
    assert!(in_mempool(&s_txid), "S floats unconfirmed in the mempool");
    eprintln!(
        "post-censor tip={tip}: D conf={} S conf={} S in_mempool={}",
        confirmations(&d_txid),
        confirmations(&s_txid),
        in_mempool(&s_txid)
    );

    // --- REVERT: confirmation-loss reconcile flips v0 complete -> pending ------
    let s2 = wm.sync_lineage(lineage_id).await.expect("censor sync");
    assert_eq!(
        s2.iter().map(|(_, s)| s.migrations_reverted).sum::<u32>(),
        1,
        "S lost its confirmation -> revert"
    );
    let (st2, tx2, h2) = read_status(&pool, v0_id).await;
    assert_eq!(st2, "pending", "v0 reverted complete -> pending");
    assert_eq!(tx2, None, "sweep txid cleared");
    assert_eq!(h2, None, "latched height cleared");
    assert!(confirmations(&d_txid) > 0, "funds preserved in D");
    eprintln!("reverted: pending, D preserved");

    // --- RED: the ordinary migration drain cannot re-sweep --------------------
    // D is spent-by-mempool (the stranded S), so drain_wallet selects nothing.
    let v0_red = wm.load_or_init(v0_id).await.expect("reload v0");
    v0_red.sync().await.expect("sync red");
    let red = v0_red.build_migration_tx(&v1_dest, FEE_SP).await;
    assert!(
        red.is_err(),
        "RED: ordinary drain must fail after the reorg (D locked by stranded S); got {red:?}"
    );
    eprintln!(
        "RED: build_migration_tx (drain) failed as expected: {}",
        red.unwrap_err()
    );

    // --- GREEN: the re-sweep builder force-respends D and RBF-displaces S ------
    let (built_sp, replaced) = v0_red
        .build_migration_resweep(&v1_dest, FEE_SP)
        .await
        .expect("GREEN: build re-sweep S'");
    assert_eq!(replaced.to_string(), s_txid, "S' replaces the stranded S");
    wm.evict_cache(v0_id).await; // drop the build-only in-memory eviction

    // Broadcast re-mark path (exactly what proposals::broadcast runs): a
    // 'resweep' proposal whose base is `pending` re-completes on broadcast.
    let proposal_id = db::insert_resweep_proposal(
        &pool,
        v0_id,
        user_id,
        migration_id,
        &built_sp.psbt_b64,
        &built_sp.proposal_json,
        &built_sp.coin_selection_json,
    )
    .await
    .expect("insert resweep proposal");
    assert_eq!(
        db::resweep_recompletion_for_proposal(&pool, proposal_id)
            .await
            .expect("recompletion query"),
        Some(v0_id),
        "a resweep on a pending base re-completes that base"
    );

    // Sign S' (2-of-3 -> 2 sigs) on a fresh handle and broadcast (RBF).
    let v0_green = wm.load_or_init(v0_id).await.expect("reload v0 for sign");
    let (sp_hex, sp_txid) = sign_2of(&v0_green, &signers, &built_sp.psbt_b64).await;
    let sp_broadcast = v0_green.broadcast_raw(&sp_hex).await.expect("broadcast S'");
    assert_eq!(sp_broadcast.to_string(), sp_txid);
    assert!(
        !in_mempool(&s_txid),
        "S' displaced S: old S left the mempool"
    );
    assert!(in_mempool(&sp_txid), "S' is now in the mempool");
    eprintln!("GREEN: S'={sp_txid} broadcast; RBF-displaced S={s_txid}");

    // Re-mark complete (broadcast handler), then it becomes idempotent.
    if let Some(base) = db::resweep_recompletion_for_proposal(&pool, proposal_id)
        .await
        .expect("recompletion query")
    {
        db::set_migration_complete(&pool, base, &sp_txid)
            .await
            .expect("re-mark complete");
    }
    assert_eq!(
        db::resweep_recompletion_for_proposal(&pool, proposal_id)
            .await
            .expect("recompletion query"),
        None,
        "re-mark is idempotent once the base is complete again"
    );

    let h_sp = mine(1); // S' confirmed
    assert!(confirmations(&sp_txid) > 0, "S' confirmed (funds in v1)");

    // --- Final reconcile: re-latch S' -----------------------------------------
    let s3 = wm.sync_lineage(lineage_id).await.expect("re-latch sync");
    assert_eq!(
        s3.iter().map(|(_, s)| s.migrations_reverted).sum::<u32>(),
        0,
        "a freshly re-completed, still-confirmed sweep latches, never reverts"
    );
    let (st3, tx3, h3) = read_status(&pool, v0_id).await;
    assert_eq!(st3, "complete", "v0 re-completed pending -> complete");
    assert_eq!(tx3.as_deref(), Some(sp_txid.as_str()), "sweep is now S'");
    assert_eq!(h3, Some(h_sp as i32), "S' confirmation height latched");
    eprintln!("re-completed: complete sweep=S' height={h3:?}  (funds in v1)");

    // --- Idempotency -----------------------------------------------------------
    let s4 = wm.sync_lineage(lineage_id).await.expect("idempotency sync");
    assert_eq!(
        s4.iter().map(|(_, s)| s.migrations_reverted).sum::<u32>(),
        0,
        "no further reverts"
    );
    let (st4, _, _) = read_status(&pool, v0_id).await;
    assert_eq!(st4, "complete", "stays complete after re-sync");

    // --- Cleanup (best-effort) ------------------------------------------------
    let _ = sqlx::query(
        "DELETE FROM transaction_proposals WHERE federation_id IN \
         (SELECT id FROM federations WHERE lineage_id = $1)",
    )
    .bind(lineage_id)
    .execute(&pool)
    .await;
    let _ = sqlx::query("DELETE FROM federation_migrations WHERE lineage_id = $1")
        .bind(lineage_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM federations WHERE lineage_id = $1")
        .bind(lineage_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await;
    eprintln!(
        "PASS: full loop — fund D, sign+broadcast S (v0->v1), latch complete; \
         censorship reorg reverted complete->pending with D preserved; RED drain \
         failed while GREEN re-sweep S' RBF-displaced S; re-marked complete, S' \
         confirmed, re-latched; funds in v1; idempotent; cleaned up"
    );
}
