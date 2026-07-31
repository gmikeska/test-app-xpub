//! Process-wide configuration loaded from environment variables (a
//! sibling `.env` file is loaded by `dotenvy` at startup if present).
//!
//! Every field is required except [`AppConfig::trezor_manifest_email`] /
//! [`AppConfig::trezor_manifest_app_url`], which are surfaced to the
//! browser for the Trezor Connect manifest and have sensible dev defaults.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use emvault::core::bitcoin::Network;
use emvault::elements::ElementsNetwork;

use emvault::config::{hex_decode, optional, require};
// Re-exported so `crate::config::ConfigError` keeps resolving across the app.
pub use emvault::config::ConfigError;

/// Which chain backend the app syncs and broadcasts through.
///
/// Selected by `APP_CHAIN_BACKEND` (default `rpc`). The two Esplora modes share
/// one `APP_ESPLORA_URL`; they differ only in scan strategy (`Waterfalls` needs
/// an enterprise/QuickSync endpoint). `Electrum` uses `APP_ELECTRUM_URL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChainBackend {
    /// Bitcoin Core JSON-RPC (`bdk_bitcoind_rpc::Emitter`).
    #[default]
    Rpc,
    /// Nodeless Esplora, address-based scan.
    Esplora,
    /// Nodeless Esplora, Waterfalls/QuickSync descriptor scan.
    Waterfalls,
    /// Descriptor-private Electrum backend (electrs/Fulcrum over
    /// `emvault::core::electrum`), selected via `APP_ELECTRUM_URL`.
    Electrum,
}

impl FromStr for ChainBackend {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rpc" => Ok(Self::Rpc),
            "esplora" => Ok(Self::Esplora),
            "waterfalls" => Ok(Self::Waterfalls),
            "electrum" => Ok(Self::Electrum),
            other => Err(ConfigError::Parse {
                var: "APP_CHAIN_BACKEND",
                reason: format!("expected rpc|esplora|waterfalls|electrum, got `{other}`"),
            }),
        }
    }
}

/// Which nodeless LWK blockchain client the **Liquid/Elements** wallet syncs and
/// broadcasts through. Selected by `ELEMENTS_CHAIN_BACKEND` (default `esplora`).
/// All three are LWK clients — there is no elementsd-RPC path in this app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ElementsChainBackend {
    /// Nodeless Esplora address-scan (`ELEMENTS_ESPLORA_URL`).
    #[default]
    Esplora,
    /// Nodeless Esplora Waterfalls descriptor scan (`ELEMENTS_ESPLORA_URL`).
    Waterfalls,
    /// Descriptor-private Electrum (`ELEMENTS_ELECTRUM_URL`).
    Electrum,
}

impl FromStr for ElementsChainBackend {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "esplora" => Ok(Self::Esplora),
            "waterfalls" => Ok(Self::Waterfalls),
            "electrum" => Ok(Self::Electrum),
            other => Err(ConfigError::Parse {
                var: "ELEMENTS_CHAIN_BACKEND",
                reason: format!("expected esplora|waterfalls|electrum, got `{other}`"),
            }),
        }
    }
}

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
    /// Whether the browser may **overwrite** an existing same-name Jade
    /// multisig registration. Off unless `ALLOW_JADE_OVERWRITE` is truthy.
    /// Safe default: the Jade driver refuses to silently replace a registration
    /// (a hostile host could otherwise swap in an attacker descriptor). Enable
    /// only for dev/testing where re-registering a federation under the same
    /// name is expected.
    pub allow_jade_overwrite: bool,
    /// Which chain backend to sync/broadcast through (`APP_CHAIN_BACKEND`,
    /// default `rpc`).
    pub chain_backend: ChainBackend,
    /// Esplora base URL (`APP_ESPLORA_URL`), required when `chain_backend` is
    /// `Esplora` or `Waterfalls`; ignored otherwise.
    pub esplora_url: Option<String>,
    /// Electrum server URL (`APP_ELECTRUM_URL`, e.g. `tcp://127.0.0.1:60001`),
    /// required when `chain_backend` is `Electrum`; ignored otherwise.
    pub electrum_url: Option<String>,
    /// Optional Liquid / Elements network. When `None`, Liquid federation
    /// creation is disabled in the UI. When `Some`, Liquid federations
    /// can be created on the configured network.
    pub elements_network: Option<ElementsNetwork>,
    /// Which backend the Liquid wallet syncs/broadcasts through
    /// (`ELEMENTS_CHAIN_BACKEND`, default `esplora`).
    pub elements_chain_backend: ElementsChainBackend,
    /// Esplora endpoint for Liquid sync (`ELEMENTS_ESPLORA_URL`). Required when
    /// `elements_chain_backend` is `Esplora` / `Waterfalls`.
    pub elements_esplora_url: Option<String>,
    /// Electrum endpoint for Liquid sync (`ELEMENTS_ELECTRUM_URL`, e.g.
    /// `tcp://10.44.0.1:60101`). Required when `elements_chain_backend` is
    /// `Electrum`.
    pub elements_electrum_url: Option<String>,
}

impl AppConfig {
    /// Read configuration from process environment.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if any required variable is missing or any
    /// value fails to parse.
    #[allow(clippy::too_many_lines)] // linear env-var reads; splitting adds no clarity
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
            optional("TREZOR_MANIFEST_EMAIL").unwrap_or_else(|| "dev@emvault.local".to_string());
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
            optional("BITCOIN_WALLET_NAME").unwrap_or_else(|| "emvault-xpub".to_string());

        // Missing or non-truthy = false, so the safe (no-overwrite) posture is
        // the default.
        let allow_jade_overwrite = optional("ALLOW_JADE_OVERWRITE").is_some_and(|v| env_truthy(&v));

        // Chain-backend selection. Default `rpc` keeps the historical
        // bitcoind-Emitter path; the nodeless/Electrum backends are opt-in.
        let chain_backend = match optional("APP_CHAIN_BACKEND") {
            Some(s) => ChainBackend::from_str(&s)?,
            None => ChainBackend::default(),
        };
        let esplora_url = optional("APP_ESPLORA_URL");
        if matches!(
            chain_backend,
            ChainBackend::Esplora | ChainBackend::Waterfalls
        ) && esplora_url.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(ConfigError::Missing {
                var: "APP_ESPLORA_URL",
            });
        }
        let electrum_url = optional("APP_ELECTRUM_URL");
        if chain_backend == ChainBackend::Electrum
            && electrum_url.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(ConfigError::Missing {
                var: "APP_ELECTRUM_URL",
            });
        }

        let elements_network = match optional("ELEMENTS_NETWORK").as_deref() {
            None => None,
            Some(s) => Some(parse_elements_network(s)?),
        };
        let elements_chain_backend = match optional("ELEMENTS_CHAIN_BACKEND") {
            Some(s) => ElementsChainBackend::from_str(&s)?,
            None => ElementsChainBackend::default(),
        };
        let elements_esplora_url = optional("ELEMENTS_ESPLORA_URL");
        let elements_electrum_url = optional("ELEMENTS_ELECTRUM_URL");
        // Only validate the indexer URL when Liquid is actually enabled.
        if elements_network.is_some() {
            match elements_chain_backend {
                ElementsChainBackend::Esplora | ElementsChainBackend::Waterfalls
                    if elements_esplora_url
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty() =>
                {
                    return Err(ConfigError::Missing {
                        var: "ELEMENTS_ESPLORA_URL",
                    });
                }
                ElementsChainBackend::Electrum
                    if elements_electrum_url
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty() =>
                {
                    return Err(ConfigError::Missing {
                        var: "ELEMENTS_ELECTRUM_URL",
                    });
                }
                _ => {}
            }
        }

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
            allow_jade_overwrite,
            chain_backend,
            esplora_url,
            electrum_url,
            elements_network,
            elements_chain_backend,
            elements_esplora_url,
            elements_electrum_url,
        })
    }

    /// The network identifier Blockstream Jade firmware expects, mapped from
    /// the configured [`Network`]. **Signet shares testnet's xpub/address
    /// versions + `tb` HRP**, so Jade treats it as `"testnet"` (confirmed in
    /// `emvault-jade-test`). Surfaced to the browser at onboarding and in the
    /// Jade sign-data payload.
    #[must_use]
    pub fn jade_network(&self) -> &'static str {
        match self.network {
            Network::Bitcoin => "mainnet",
            Network::Regtest => "localtest",
            // Testnet + Signet (which shares testnet versions), plus any future
            // testnet-like variant of the `#[non_exhaustive]` `Network` enum.
            _ => "testnet",
        }
    }
}

// `require`, `optional`, `hex_decode`, and `ConfigError` now live in
// `emvault::config` (imported above) — deduplicated in extraction phase E5b.

/// Interpret an env-var value as a boolean flag. Truthy = `1`/`true`/`yes`/`on`
/// (case-insensitive, surrounding whitespace ignored); everything else is
/// false. Kept as a free function so the parsing can be unit-tested without
/// touching process-global environment state.
fn env_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_elements_network(s: &str) -> Result<ElementsNetwork, ConfigError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "liquid" => Ok(ElementsNetwork::Liquid),
        "liquidtestnet" | "liquid_testnet" => Ok(ElementsNetwork::LiquidTestnet),
        "elementsregtest" | "elements_regtest" => Ok(ElementsNetwork::ElementsRegtest),
        other => Err(ConfigError::Parse {
            var: "ELEMENTS_NETWORK",
            reason: format!(
                "expected one of `liquid`, `liquidtestnet`, `elementsregtest`; got `{other}`"
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::env_truthy;

    #[test]
    fn env_truthy_accepts_common_true_spellings() {
        for v in [
            "1", "true", "TRUE", "True", "yes", "YES", "on", "ON", "  true  ",
        ] {
            assert!(env_truthy(v), "{v:?} should be truthy");
        }
    }

    #[test]
    fn env_truthy_rejects_everything_else() {
        for v in [
            "", " ", "0", "false", "no", "off", "2", "t", "y", "enabled", "null",
        ] {
            assert!(!env_truthy(v), "{v:?} should be falsey");
        }
    }
}
