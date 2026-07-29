//! **Read-only diagnostic** (not a permanent regression) — loads every persisted
//! federation whose `migration_status = 'complete'` from the live `asterism_xpub`
//! DB, inspects its BDK tx-graph anchors for *orphaned* anchors (an anchor whose
//! block is absent from the wallet's own local chain — the frozen fingerprint of
//! an absorbed reorg), and runs the **current** `chain_sync::emitter_sync` against
//! the live node **in memory only** (never persists) to observe the firing branch.
//!
//! This is the ground-truth reproduction of the production detection miss Maggie
//! flagged: a wallet whose reorg damage is already baked into the persisted
//! changeset. Because the reorged-out tx canonicalizes as *unconfirmed the moment
//! the changeset is reloaded*, `emitter_sync`'s same-pass confirmed→unconfirmed
//! diff has nothing to compare against and returns `reorg_rebuilt = false` forever.
//! The orphaned-anchor signal (printed here) is what a robust detector keys on.
//!
//! Gate: `RPC_LIVE=1` (needs gv-regtest bitcoind :18543 + Postgres :5546). Run:
//! ```bash
//! RPC_LIVE=1 cargo test --test reorg_baked_diag -- --nocapture --test-threads=1
//! ```

#![allow(clippy::too_many_lines)]

use emvault::core::bitcoin::Network;
use emvault::core::bitcoincore_rpc::{Auth, Client as RpcClient};
use emvault::core::chain_sync::{self};
use sqlx::Row;

fn skip() -> bool {
    if std::env::var("RPC_LIVE").ok().as_deref() != Some("1") {
        eprintln!("SKIP: set RPC_LIVE=1 (needs gv-regtest bitcoind + Postgres)");
        return true;
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnose_baked_reorg_wallets() {
    if skip() {
        return;
    }
    let db_url = "postgres://asterism:asterism@host.docker.internal:5546/asterism_xpub";
    let rpc_url = "http://host.docker.internal:18543";

    let pool = sqlx::PgPool::connect(db_url)
        .await
        .expect("connect Postgres");
    let rpc = RpcClient::new(
        rpc_url,
        Auth::UserPass("regtest".to_string(), "regtest".to_string()),
    )
    .expect("rpc client");

    let rows = sqlx::query(
        "SELECT id, label, network, descriptor, bdk_changeset, chain_tip_height, \
         migration_status, migration_sweep_txid \
         FROM federations WHERE migration_status = 'complete' ORDER BY chain_tip_height",
    )
    .fetch_all(&pool)
    .await
    .expect("query complete-migration federations");

    eprintln!(
        "found {} federation(s) with migration_status=complete",
        rows.len()
    );

    for row in &rows {
        let id: uuid::Uuid = row.get("id");
        let label: String = row.get("label");
        let network: String = row.get("network");
        let descriptor: String = row.get("descriptor");
        let changeset: Option<serde_json::Value> = row.get("bdk_changeset");
        let tip: Option<i32> = row.get("chain_tip_height");
        let sweep: Option<String> = row.get("migration_sweep_txid");
        let net = <Network as std::str::FromStr>::from_str(&network).unwrap_or(Network::Regtest);

        eprintln!("\n=== {id} [{label}] net={network} persisted_tip={tip:?} sweep={sweep:?} ===");

        let loaded = chain_sync::init_or_load_wallet(net, descriptor, changeset)
            .expect("load persisted wallet");
        let mut wallet = loaded.wallet;

        // --- Orphaned-anchor inspection (the robust reorg signal) --------------
        let chain = wallet.local_chain();
        let mut orphan_anchors = 0usize;
        for (txid, set) in wallet.tx_graph().all_anchors() {
            for cbt in set {
                let bid = cbt.block_id;
                let on_chain = chain.get(bid.height).map(|cp| cp.hash());
                if on_chain != Some(bid.hash) {
                    orphan_anchors += 1;
                    eprintln!(
                        "  ORPHAN anchor: tx {txid} @ block ({}, {}) — local_chain has {:?}",
                        bid.height, bid.hash, on_chain
                    );
                }
            }
        }
        // Does the loaded wallet already regard the recorded sweep as absent?
        let sweep_present = sweep.as_deref().and_then(|s| s.parse().ok()).is_some_and(
            |want: emvault::core::bitcoin::Txid| {
                wallet.transactions().any(|w| w.tx_node.txid == want)
            },
        );
        eprintln!(
            "  orphan_anchors={orphan_anchors} recorded_sweep_present_in_graph={sweep_present}"
        );

        // --- Run the CURRENT emitter_sync in memory (NO persist) --------------
        let result = chain_sync::emitter_sync(&mut wallet, &rpc).expect("emitter_sync");
        eprintln!(
            "  CURRENT emitter_sync => reorg_rebuilt={} evicted={} tip={}",
            result.reorg_rebuilt,
            result.evicted_txids.len(),
            result.tip_height
        );
        if orphan_anchors > 0 && !result.reorg_rebuilt {
            eprintln!(
                "  >>> MISS CONFIRMED: {orphan_anchors} orphaned anchor(s) present, yet \
                 reorg_rebuilt=false — the baked-in reorg is undetected."
            );
        } else if orphan_anchors > 0 && result.reorg_rebuilt {
            eprintln!("  >>> DETECTED: emitter_sync rebuilt the baked-in reorg (fix active).");
        }
    }
    eprintln!("\n(diagnostic only — no rows were modified)");
}
