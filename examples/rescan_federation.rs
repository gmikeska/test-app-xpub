//! Rescan a federation's wallet from a chosen height (default genesis) to the
//! node tip, then persist the result so the running app sees any recovered
//! funds.
//!
//! Developer accommodation — the complement to the birthday-bootstrap in the
//! web app: birthday = fast start (no funds predate creation); rescan = recovery
//! when that assumption breaks (a dev DB reset, or coins sent to a federation's
//! addresses before it was tracked). The federation descriptor is deterministic,
//! so the same members → the same addresses → a from-zero scan rediscovers them.
//!
//! ```text
//! cargo run --example rescan_federation -- --federation <uuid>
//! cargo run --example rescan_federation -- --federation <uuid> --from 250000
//! ```
//!
//! Reads the same `.env` as the web app (DATABASE_URL, BITCOIN_RPC_*,
//! BITCOIN_NETWORK). Always **persists** the rescanned changeset. A full
//! from-zero scan on signet/mainnet is slow (one `getblock` RPC per block); pass
//! `--from <height>` near the deposit to bound it. While scanning it shows a
//! single, in-place progress line (current block + percentage); on completion it
//! prints a report and a banner.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bitcoin::Amount;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use test_app_xpub::config::AppConfig;
use test_app_xpub::wallet::{RescanReport, WalletManager};

struct Args {
    /// `None` → rescan **every** federation.
    federation_id: Option<Uuid>,
    from_height: u32,
}

fn print_usage() {
    eprintln!(
        "\
Usage: rescan_federation [--federation <uuid>] [--from <height>]

Rebuilds a federation's BDK wallet from its descriptor and scans the chain from
<height> (default 0 = genesis) to the node tip, then persists the result so the
web app sees any recovered funds.

Options:
  --federation <uuid>   Federation id to rescan. If omitted, rescans EVERY
                        federation (all versions, all lineages).
  --from <height>       Start block height (default 0). Bound it if you know
                        roughly when funds were sent — a from-zero signet scan
                        fetches every block over RPC.
  --help                Show this help."
    );
}

fn parse_args() -> Result<Args, String> {
    let mut federation_id: Option<Uuid> = None;
    let mut from_height: u32 = 0;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--federation" | "-f" => {
                let v = it.next().ok_or("--federation requires a <uuid>")?;
                federation_id =
                    Some(Uuid::parse_str(&v).map_err(|e| format!("invalid federation uuid: {e}"))?);
            }
            "--from" => {
                let v = it.next().ok_or("--from requires a <height>")?;
                from_height = v.parse().map_err(|e| format!("invalid --from height: {e}"))?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        federation_id, // None → rescan all
        from_height,
    })
}

#[tokio::main]
async fn main() {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    let _ = dotenvy::from_path(&env_path);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("test_app_xpub=info")),
        )
        .init();

    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });

    let config = AppConfig::from_env().unwrap_or_else(|e| {
        eprintln!("error: config: {e}");
        std::process::exit(1);
    });

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(8))
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: connect Postgres: {e}");
            std::process::exit(1);
        });

    let wallets = Arc::new(WalletManager::new(pool, &config).unwrap_or_else(|e| {
        eprintln!("error: build wallet manager: {e}");
        std::process::exit(1);
    }));

    if args.from_height == 0 {
        println!("(from genesis — this fetches every block over RPC and may take a while)");
    }
    let started = Instant::now();

    match args.federation_id {
        Some(id) => run_single(&wallets, id, args.from_height, &config, started).await,
        None => run_all(&wallets, args.from_height, &config, started).await,
    }
}

/// Rescan one federation and print the full report.
async fn run_single(
    wallets: &WalletManager,
    id: Uuid,
    from_height: u32,
    config: &AppConfig,
    started: Instant,
) {
    println!(
        "Rescanning federation {} from height {} on {} (node {})…",
        id, from_height, config.network, config.bitcoin_rpc_url
    );
    let result = wallets
        .rescan(id, from_height, |height, tip| {
            print_progress("", height, tip, from_height);
        })
        .await;
    println!(); // close the in-place progress line
    match result {
        Ok(report) => {
            print_report(&report, started.elapsed());
            notify_complete(&format!(
                "rescan complete — {} ({:.8} BTC, {} UTXOs)",
                report.label,
                report.balance.total().to_btc(),
                report.utxo_count
            ));
        }
        Err(e) => {
            eprintln!("\nerror: rescan failed: {e}");
            notify_complete("rescan FAILED");
            std::process::exit(1);
        }
    }
}

/// Rescan every federation; print one compact line each + a totals summary.
async fn run_all(wallets: &WalletManager, from_height: u32, config: &AppConfig, started: Instant) {
    println!(
        "Rescanning ALL federations from height {} on {} (node {})…",
        from_height, config.network, config.bitcoin_rpc_url
    );
    let ids = match wallets.federation_ids().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("\nerror: listing federations failed: {e}");
            notify_complete("rescan-all FAILED");
            std::process::exit(1);
        }
    };
    if ids.is_empty() {
        println!("  (no federations in the database)");
        notify_complete("rescan-all complete — 0 federations");
        return;
    }

    // Scan each federation in turn, sharing a single in-place progress line per
    // federation; replace it with the compact result row when each one finishes.
    let n = ids.len();
    let (mut ok, mut failed, mut total) = (0usize, 0usize, Amount::ZERO);
    for (i, id) in ids.iter().enumerate() {
        let sid = id.to_string();
        let prefix = format!("[{}/{}] {}… ", i + 1, n, &sid[..8]);
        let res = wallets
            .rescan(*id, from_height, |height, tip| {
                print_progress(&prefix, height, tip, from_height);
            })
            .await;
        print!("\r\x1b[2K"); // clear the progress line before the result row
        match res {
            Ok(r) => {
                ok += 1;
                total += r.balance.total();
                println!(
                    "  ✓ {:<24} {:.8} BTC · {:>2} UTXO(s)  (scanned {}..{})",
                    truncate(&r.label, 24),
                    r.balance.total().to_btc(),
                    r.utxo_count,
                    r.from_height,
                    r.tip_height
                );
            }
            Err(e) => {
                failed += 1;
                println!("  ✗ {id}  {e}");
            }
        }
    }
    println!(
        "\n  {ok} rescanned, {failed} failed · total {:.8} BTC · {:.1}s",
        total.to_btc(),
        started.elapsed().as_secs_f64()
    );
    println!("  Persisted — reload the federations in the app.");
    notify_complete(&format!(
        "rescan-all complete — {ok} federations, {:.8} BTC{}",
        total.to_btc(),
        if failed > 0 {
            format!(", {failed} FAILED")
        } else {
            String::new()
        }
    ));
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Render a single, in-place progress line (carriage-return, no newline) showing
/// the current block and percentage through the `from..tip` range. The caller
/// terminates the line (a newline, or an ANSI clear) once the scan finishes.
fn print_progress(prefix: &str, height: u32, tip: u32, from: u32) {
    let span = tip.saturating_sub(from).max(1);
    let done = height.saturating_sub(from);
    let pct = (f64::from(done) / f64::from(span) * 100.0).clamp(0.0, 100.0);
    print!("\r  {prefix}block {height} / {tip}  ({pct:5.1}%)");
    let _ = std::io::stdout().flush();
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn print_report(r: &RescanReport, elapsed: Duration) {
    let b = &r.balance;
    println!("\n  Rescan Report");
    println!("  ─────────────");
    println!("  Federation:  {} ({})", r.label, r.federation_id);
    println!(
        "  Scanned:     {}..{}  ({} blocks, {:.1}s)",
        r.from_height,
        r.tip_height,
        r.blocks_scanned,
        elapsed.as_secs_f64()
    );
    println!("  Balance:     {:.8} BTC total", b.total().to_btc());
    println!(
        "               confirmed {:.8} · pending {:.8} · immature {:.8}",
        b.confirmed.to_btc(),
        (b.trusted_pending + b.untrusted_pending).to_btc(),
        b.immature.to_btc()
    );
    println!("  Unspent:     {} UTXO(s)", r.utxo_count);
    print_addr_table("Funded receive addresses", &r.receive_addresses);
    print_addr_table("Funded change addresses", &r.change_addresses);
    println!("\n  Persisted the rescanned changeset — reload the federation in the app.");
}

fn print_addr_table(title: &str, addrs: &[test_app_xpub::wallet::RevealedAddress]) {
    if addrs.is_empty() {
        return;
    }
    println!("\n  {title}:");
    for a in addrs {
        println!(
            "    [{:>3}] {}  received {:.8} · unspent {:.8}",
            a.index,
            a.address,
            a.received.to_btc(),
            a.unspent.to_btc()
        );
    }
}

/// Print a prominent banner so a long, unattended run is noticeable when it
/// finishes. (No terminal bell — the caller has their own notification setup.)
fn notify_complete(msg: &str) {
    println!("\n========================================");
    println!("✅ {msg}");
    println!("========================================");
}
