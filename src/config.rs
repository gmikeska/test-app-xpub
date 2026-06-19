//! Process-wide configuration loaded from environment variables (a
//! sibling `.env` file is loaded by `dotenvy` at startup if present).
//!
//! Every field is required except [`AppConfig::trezor_manifest_email`] /
//! [`AppConfig::trezor_manifest_app_url`], which are surfaced to the
//! browser for the Trezor Connect manifest and have sensible dev defaults.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use bitcoin::Network;

/// Top-level configuration for the web app.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Where the HTTP server binds.
    pub bind: SocketAddr,
    /// Session cookie signing key (hex, decoded into bytes at startup).
    pub session_secret: Vec<u8>,
    /// `PostgreSQL` connection string.
    pub database_url: String,
    /// Bitcoin network every onboarded signer must agree with.
    pub network: Network,
    /// BIP-48 derivation path browser code requests from Trezor.
    pub federation_derivation_path: String,
    /// Trezor Connect `coin` token: `"btc"` (mainnet) or `"test"` (testnet).
    pub trezor_coin: String,
    /// Trezor Connect manifest contact email.
    pub trezor_manifest_email: String,
    /// Trezor Connect manifest origin URL.
    pub trezor_manifest_app_url: String,
    /// Bitcoin Core JSON-RPC base URL, e.g. `http://127.0.0.1:18443`.
    pub bitcoin_rpc_url: String,
    /// Bitcoin Core RPC username.
    pub bitcoin_rpc_user: String,
    /// Bitcoin Core RPC password.
    pub bitcoin_rpc_password: String,
    /// Name passed to Bitcoin Core's `loadwallet` when needed.
    ///
    /// Currently unused by the BDK descriptor wallet path, but kept for
    /// future RPC calls that require a wallet context.
    pub bitcoin_wallet_name: String,
}

impl AppConfig {
    /// Read configuration from process environment.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if any required variable is missing or any
    /// value fails to parse.
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = require("APP_HOST")?;
        let port: u16 = require("APP_PORT")?
            .parse()
            .map_err(|e: std::num::ParseIntError| ConfigError::Parse {
                var: "APP_PORT",
                reason: e.to_string(),
            })?;
        let host_ip: IpAddr =
            host.parse()
                .map_err(|e: std::net::AddrParseError| ConfigError::Parse {
                    var: "APP_HOST",
                    reason: e.to_string(),
                })?;

        let secret_hex = require("APP_SESSION_SECRET")?;
        let session_secret = hex_decode(&secret_hex).map_err(|reason| ConfigError::Parse {
            var: "APP_SESSION_SECRET",
            reason,
        })?;
        if session_secret.len() < 64 {
            return Err(ConfigError::Parse {
                var: "APP_SESSION_SECRET",
                reason: format!(
                    "session secret must be at least 64 bytes (got {})",
                    session_secret.len()
                ),
            });
        }

        let database_url = require("DATABASE_URL")?;

        let network_str = require("BITCOIN_NETWORK")?;
        let network = Network::from_str(&network_str).map_err(|e| ConfigError::Parse {
            var: "BITCOIN_NETWORK",
            reason: e.to_string(),
        })?;

        let federation_derivation_path = require("APP_FED_DERIVATION_PATH")?;
        let trezor_coin = require("TREZOR_COIN")?;
        let trezor_manifest_email =
            optional("TREZOR_MANIFEST_EMAIL").unwrap_or_else(|| "dev@asterism.local".to_string());
        let trezor_manifest_app_url = optional("TREZOR_MANIFEST_APP_URL")
            .unwrap_or_else(|| format!("http://{host_ip}:{port}"));

        let rpc_host = require("BITCOIN_RPC_HOST")?;
        let rpc_port: u16 =
            require("BITCOIN_RPC_PORT")?
                .parse()
                .map_err(|e: std::num::ParseIntError| ConfigError::Parse {
                    var: "BITCOIN_RPC_PORT",
                    reason: e.to_string(),
                })?;
        let bitcoin_rpc_url = format!("http://{rpc_host}:{rpc_port}");
        let bitcoin_rpc_user = require("BITCOIN_RPC_USER")?;
        let bitcoin_rpc_password = require("BITCOIN_RPC_PASSWORD")?;
        let bitcoin_wallet_name =
            optional("BITCOIN_WALLET_NAME").unwrap_or_else(|| "asterism-xpub".to_string());

        Ok(Self {
            bind: SocketAddr::new(host_ip, port),
            session_secret,
            database_url,
            network,
            federation_derivation_path,
            trezor_coin,
            trezor_manifest_email,
            trezor_manifest_app_url,
            bitcoin_rpc_url,
            bitcoin_rpc_user,
            bitcoin_rpc_password,
            bitcoin_wallet_name,
        })
    }
}

fn require(var: &'static str) -> Result<String, ConfigError> {
    std::env::var(var).map_err(|_| ConfigError::Missing { var })
}

fn optional(var: &'static str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.is_empty())
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err(format!("odd-length hex string ({} chars)", s.len()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| format!("invalid hex at byte {}: {e}", i / 2))
        })
        .collect()
}

/// Configuration loading errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required environment variable was not set.
    #[error("required env var `{var}` is not set")]
    Missing {
        /// The variable name.
        var: &'static str,
    },
    /// A variable was set but failed to parse.
    #[error("env var `{var}` is invalid: {reason}")]
    Parse {
        /// The variable name.
        var: &'static str,
        /// Human-readable reason.
        reason: String,
    },
}
