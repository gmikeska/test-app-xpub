//! Live e2e — **Elements / Liquid asset features** through `test-app-xpub`'s real
//! LWK wallet + proposal plumbing, signed in software (no Jade required).
//!
//! This is the Liquid counterpart of the `reorg_*_e2e` Bitcoin tests. It builds a
//! genuine 2-of-3 confidential federation from `emvault-elements`'
//! [`SoftwareSigner`]s (which own their keys and sign PSETs in-process over the
//! Elements segwit-v0 sighash), funds it on a **regtest** Elements node with both
//! L-BTC and an **issued asset**, then drives the app's own code paths end to end:
//!
//!   * `asset_balances` / `address_activity_by_asset` — the per-asset accounting
//!     the Receive/Holdings UI reads (policy asset first, issued assets after);
//!   * `build_proposal(asset)` — an **issued-asset send** proposal, signed by 2 of
//!     3 software signers → `merge_partial_pset` → `finalize_and_extract` →
//!     `broadcast_raw`, then re-synced to prove the asset left the wallet;
//!   * `build_migration_pset` — the **asset-aware migration sweep**, asserting its
//!     `assets_swept` payload lists the issued asset, then signed/finalized/
//!     broadcast so the successor address ends up holding **both** L-BTC and the
//!     asset in one transaction.
//!
//! ## Harness (elements-regtest)
//!   * elementsd — JSON-RPC at `ELEMENTS_RPC_HOST:ELEMENTS_RPC_PORT`
//!     (`ELEMENTS_RPC_USER`/`ELEMENTS_RPC_PASSWORD`, wallet `ELEMENTS_WALLET_NAME`)
//!     with a spendable L-BTC balance and block generation on demand.
//!   * An Electrum **or** Esplora indexer over that node (`ELEMENTS_ELECTRUM_URL`
//!     / `ELEMENTS_ESPLORA_URL`), so the LWK wollet can sync.
//!   * Postgres — `DATABASE_URL` (schema auto-migrated).
//!
//! `SoftwareSigner`'s sighash mixes in the network genesis, so `ELEMENTS_NETWORK`
//! **must** match the node (an `elementsregtest` node with a non-default genesis
//! needs the matching custom-regtest network, or signatures won't finalize).
//!
//! No hardware is touched. Opt-in gate (skips cleanly otherwise): `RPC_LIVE=1`:
//! ```bash
//! RPC_LIVE=1 cargo test --test elements_asset_e2e -- --nocapture --test-threads=1
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
use std::str::FromStr;

use serde_json::{Value, json};
use uuid::Uuid;

use emvault::elements::elements::pset::PartiallySignedTransaction as Pset;
use emvault::elements::testkit::SoftwareSigner;
use emvault::elements::{CtDescriptorBuilder, ElementsSigner};

use test_app_xpub::config::AppConfig;
use test_app_xpub::db::{self, NewFederation};
use test_app_xpub::elements_wallet::LwkWalletManager;

/// A regtest issued asset amount (in sats, 8-dp): 1000.00000000 units.
const ASSET_SATS: u64 = 100_000_000_000;
/// L-BTC funded to the federation, enough to send + pay fees.
const LBTC_SATS: u64 = 5_000_000; // 0.05 L-BTC

// ---------------------------------------------------------------------------
// Gate + env
// ---------------------------------------------------------------------------

fn skip() -> bool {
    if std::env::var("RPC_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skipping elements_asset_e2e: set RPC_LIVE=1 (needs elements-regtest + indexer)");
        return true;
    }
    false
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// ---------------------------------------------------------------------------
// elementsd JSON-RPC over curl (mirrors the bitcoind helper in the reorg e2e)
// ---------------------------------------------------------------------------

fn ecli(method: &str, params: Value) -> Value {
    let host = env_or("ELEMENTS_RPC_HOST", "host.docker.internal");
    let port = env_or("ELEMENTS_RPC_PORT", "18884");
    let user = env_or("ELEMENTS_RPC_USER", "elements");
    let pass = env_or("ELEMENTS_RPC_PASSWORD", "elementspass");
    let wallet = env_or("ELEMENTS_WALLET_NAME", "default");
    let url = format!("http://{host}:{port}/wallet/{wallet}");
    let body = json!({ "jsonrpc": "1.0", "id": "e2e", "method": method, "params": params });
    let out = Command::new("curl")
        .args([
            "-s",
            "--user",
            &format!("{user}:{pass}"),
            "-H",
            "content-type: text/plain",
            "--data-binary",
            &body.to_string(),
            &url,
        ])
        .output()
        .expect("spawn curl");
    let resp: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "elements RPC {method}: non-JSON: {:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert!(
        resp.get("error").is_none_or(Value::is_null),
        "elements RPC {method} error: {}",
        resp["error"]
    );
    resp["result"].clone()
}

/// Mine `n` blocks to a throwaway node address (confirms broadcasts).
fn mine(n: u64) {
    let addr = ecli("getnewaddress", json!([]));
    let addr = addr.as_str().expect("getnewaddress");
    ecli("generatetoaddress", json!([n, addr]));
}

/// Issue a fresh regtest asset and return its hex id (blinded issuance to the
/// node's own wallet); mines it in.
fn issue_asset(units: f64) -> String {
    let res = ecli("issueasset", json!([units, 0])); // 0 reissuance tokens
    mine(1);
    res["asset"].as_str().expect("issueasset.asset").to_string()
}

/// Send `amount_sat` of `asset` (policy L-BTC when `asset` is `None`) to `addr`,
/// mine it, and return the txid.
fn fund(addr: &str, amount_sat: u64, asset: Option<&str>) -> String {
    let amount = amount_sat as f64 / 1e8;
    let txid = if let Some(a) = asset {
        // -named-style call: address, amount, comment, comment_to, subtractfee,
        // replaceable, conf_target, estimate_mode, avoid_reuse, assetlabel.
        ecli(
            "sendtoaddress",
            json!([addr, amount, "", "", false, true, 1, "unset", false, a]),
        )
    } else {
        ecli("sendtoaddress", json!([addr, amount]))
    };
    mine(1);
    txid.as_str().expect("sendtoaddress txid").to_string()
}

// ---------------------------------------------------------------------------
// Federation provisioning (software signers → CT descriptor → DB row)
// ---------------------------------------------------------------------------

/// A 2-of-3 confidential federation built from three deterministic software
/// signers. Returns the federation id and the three signers (kept so the test
/// can sign the PSETs it builds).
async fn provision(
    pool: &sqlx::PgPool,
    config: &AppConfig,
    label: &str,
) -> (Uuid, Vec<SoftwareSigner>, [u8; 32]) {
    let lwk_net = config
        .elements_network
        .expect("ELEMENTS_NETWORK must be set for the elements e2e")
        .to_lwk();
    let signers: Vec<SoftwareSigner> = (1u8..=3)
        .map(|seed| SoftwareSigner::new_with_lwk(seed, lwk_net))
        .collect();

    // A fixed SLIP-77 master blinding key for the confidential descriptor.
    let mbk: [u8; 32] = [0x2a; 32];
    let mut builder = CtDescriptorBuilder::new(2, &mbk).expect("ct descriptor builder");
    for s in &signers {
        builder.add_signer(s).expect("add signer");
    }
    let descriptor = builder.build().expect("build ct descriptor").to_string();

    let network = env_or("ELEMENTS_NETWORK", "elementsregtest");
    let spec = NewFederation {
        label,
        threshold: 2,
        total_signers: 3,
        network: &network,
        descriptor: &descriptor,
        script_type: "wsh",
        nums_chaincode: None,
        elements_descriptor: Some(&descriptor),
        snapshot_json: &json!({ "kind": "elements-e2e", "label": label }),
        master_blinding_key: Some(&mbk),
    };
    let fed_id = db::insert_federation_with_members(pool, &spec, &[])
        .await
        .expect("insert elements federation");
    (fed_id, signers, mbk)
}

/// Sign `pset_b64` with `signers` (2 of 3 needed), merging each partial into the
/// base via the app's `merge_partial_pset`, then finalize + broadcast through the
/// app. Returns the broadcast txid.
async fn sign_finalize_broadcast(
    wallet: &test_app_xpub::elements_wallet::LiquidFederationWallet,
    pset_b64: &str,
    signers: &[SoftwareSigner],
) -> String {
    let mut base_b64 = pset_b64.to_string();
    let mut fully_signed = false;
    for s in signers {
        let mut pset = Pset::from_str(pset_b64).expect("parse base pset for signer");
        let n = s.sign_pset(&mut pset).expect("sign_pset");
        assert!(n > 0, "software signer produced no signatures");
        let partial_b64 = pset.to_string();
        let merged = wallet
            .merge_partial_pset(&base_b64, &partial_b64)
            .await
            .expect("merge partial pset");
        base_b64 = merged.merged_pset_b64;
        fully_signed = merged.fully_signed;
        if fully_signed {
            break;
        }
    }
    assert!(fully_signed, "PSET never reached the signing threshold");

    let finalized = wallet
        .finalize_and_extract(&base_b64)
        .await
        .expect("finalize + extract");
    let txid = wallet
        .broadcast_raw(&finalized.tx_hex)
        .await
        .expect("broadcast raw");
    mine(2);
    txid
}

fn config_from_env() -> AppConfig {
    AppConfig::from_env().expect("elements AppConfig::from_env (needs ELEMENTS_* + DATABASE_URL)")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Receive L-BTC + an issued asset and assert the per-asset accounting the UI
/// reads: `asset_balances` lists the policy asset first with the right totals,
/// and `address_activity_by_asset` attributes each asset to the funded address.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn elements_asset_receive_and_balances() {
    if skip() {
        return;
    }
    let config = config_from_env();
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations");
    let mgr = LwkWalletManager::new(pool.clone(), &config);

    let label = format!("elem-recv-{}", Uuid::new_v4());
    let (fed_id, _signers, _mbk) = provision(&pool, &config, &label).await;
    let wallet = mgr
        .load_or_init(fed_id)
        .await
        .expect("load elements wallet");
    wallet.sync().await.expect("initial sync");

    let addr = wallet
        .reveal_addresses(1)
        .await
        .expect("reveal address")
        .into_iter()
        .next()
        .expect("one address")
        .address;

    let asset = issue_asset(1000.0);
    fund(&addr, LBTC_SATS, None);
    fund(&addr, ASSET_SATS, Some(&asset));
    wallet.sync().await.expect("resync after funding");

    // Holdings-by-asset: policy asset (L-BTC) first, then the issued asset.
    let balances = wallet.asset_balances().await.expect("asset balances");
    assert!(!balances.is_empty(), "expected at least L-BTC");
    assert!(balances[0].is_policy, "policy asset must be listed first");
    let lbtc = balances.iter().find(|b| b.is_policy).expect("L-BTC row");
    assert_eq!(lbtc.sat, LBTC_SATS, "L-BTC balance");
    let issued = balances
        .iter()
        .find(|b| b.asset_id == asset)
        .expect("issued asset row present in holdings");
    assert!(!issued.is_policy);
    assert_eq!(issued.sat, ASSET_SATS, "issued-asset balance");

    // asset_total_sat is a wallet-wide convenience over the same data.
    assert_eq!(
        wallet
            .asset_total_sat(&asset)
            .await
            .expect("asset_total_sat"),
        ASSET_SATS
    );
}

/// Build an **issued-asset send** proposal, sign it 2-of-3 in software, finalize
/// and broadcast, and prove the asset left the wallet.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn elements_asset_send_e2e() {
    if skip() {
        return;
    }
    let config = config_from_env();
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations");
    let mgr = LwkWalletManager::new(pool.clone(), &config);

    let label = format!("elem-send-{}", Uuid::new_v4());
    let (fed_id, signers, _mbk) = provision(&pool, &config, &label).await;
    let wallet = mgr
        .load_or_init(fed_id)
        .await
        .expect("load elements wallet");
    wallet.sync().await.expect("initial sync");
    let addr = wallet.reveal_addresses(1).await.expect("reveal")[0]
        .address
        .clone();

    let asset = issue_asset(1000.0);
    fund(&addr, LBTC_SATS, None);
    fund(&addr, ASSET_SATS, Some(&asset));
    wallet.sync().await.expect("resync");
    assert_eq!(wallet.asset_total_sat(&asset).await.unwrap(), ASSET_SATS);

    // Send half the asset back to the node.
    let dest_raw = ecli("getnewaddress", json!([]));
    let dest = wallet
        .parse_address(dest_raw.as_str().unwrap())
        .expect("parse dest");
    let half = ASSET_SATS / 2;
    let built = wallet
        .build_proposal(&dest, half, Some(&asset), Some(1))
        .await
        .expect("build asset send proposal");
    // The proposal JSON records the actual asset, not L-BTC.
    assert_eq!(built.proposal_json["asset"].as_str(), Some(asset.as_str()));

    sign_finalize_broadcast(&wallet, &built.pset_b64, &signers).await;
    wallet.sync().await.expect("resync after send");

    // The wallet should now hold (at most) the un-sent remainder of the asset.
    let remaining = wallet.asset_total_sat(&asset).await.expect("asset total");
    assert!(
        remaining <= half,
        "expected ≤ half the asset to remain after the send, got {remaining}"
    );
}

/// The **asset-aware migration**: build the sweep, assert its `assets_swept`
/// payload carries the issued asset, sign/finalize/broadcast it, and prove the
/// successor address ends up holding both L-BTC and the asset (one transaction).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn elements_asset_aware_migration_e2e() {
    if skip() {
        return;
    }
    let config = config_from_env();
    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .expect("connect Postgres");
    db::migrate(&pool).await.expect("migrations");
    let mgr = LwkWalletManager::new(pool.clone(), &config);

    // Source federation (holds the funds) + a successor federation (destination).
    let (src_id, signers, _mbk) =
        provision(&pool, &config, &format!("elem-mig-src-{}", Uuid::new_v4())).await;
    let (dst_id, _dst_signers, _dst_mbk) =
        provision(&pool, &config, &format!("elem-mig-dst-{}", Uuid::new_v4())).await;

    let src = mgr.load_or_init(src_id).await.expect("load src");
    let dst = mgr.load_or_init(dst_id).await.expect("load dst");
    src.sync().await.expect("src sync");
    dst.sync().await.expect("dst sync");

    let src_addr = src.reveal_addresses(1).await.expect("src addr")[0]
        .address
        .clone();
    let asset = issue_asset(1000.0);
    fund(&src_addr, LBTC_SATS, None);
    fund(&src_addr, ASSET_SATS, Some(&asset));
    src.sync().await.expect("src resync");

    // Successor's first CT address = the migration sweep destination.
    let dst_addr_raw = dst.reveal_addresses(1).await.expect("dst addr")[0]
        .address
        .clone();
    let destination = src.parse_address(&dst_addr_raw).expect("parse dst addr");

    let sweep = src
        .build_migration_pset(&destination, Some(1))
        .await
        .expect("build asset-aware migration sweep");
    // The sweep must advertise the issued asset in assets_swept.
    let swept = sweep.proposal_json["assets_swept"]
        .as_array()
        .expect("assets_swept array");
    assert!(
        swept
            .iter()
            .any(|a| a["asset"].as_str() == Some(asset.as_str())),
        "migration sweep must include the issued asset in assets_swept"
    );

    sign_finalize_broadcast(&src, &sweep.pset_b64, &signers).await;
    src.sync().await.expect("src resync post-sweep");
    dst.sync().await.expect("dst resync post-sweep");

    // Source emptied; successor holds both L-BTC and the asset.
    assert_eq!(
        src.asset_total_sat(&asset).await.unwrap(),
        0,
        "src asset swept"
    );
    assert_eq!(
        dst.asset_total_sat(&asset).await.unwrap(),
        ASSET_SATS,
        "successor received the full asset"
    );
    assert!(
        dst.lbtc_balance_sat().await.unwrap() > 0,
        "successor received the L-BTC too"
    );
}
