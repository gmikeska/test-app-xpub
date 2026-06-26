//! Federation-migration sweep planning for `test-app-xpub` (Phase 0).
//!
//! The xpub app is **single-account** (BIP-48 account `0'`). A migration moves
//! the current federation version's funds to its successor in a **single
//! transaction**, with the fee paid from the treasury itself —
//! [`AccountForAccountSweep`] with account `0` designated as the fee account.
//! In this degenerate (one-account) case the sweep's sole output is a
//! [`SweepOutput::FeeChange`] for account `0` carrying `balance - fee`, with
//! `is_fee_final = true` (so the consumer routes it to the new federation's
//! address when it builds the actual PSBT).
//!
//! Using `AccountForAccountSweep` here (rather than a flat consolidation) means
//! the same path generalises unchanged the moment a per-member account model
//! lands: N accounts, with the *initiator's* account as `fee_account_idx`. See
//! `emerald_multisignature/xpub_federation_migration.md` §5.2 and the plan
//! `plans/xpub-federation-migration.md` (Phase 0). The deferred per-member-fee
//! model is tracked in `emerald_multisignature/TODO.md`.
//!
//! The `plan_*` entry point and [`MigrationSweep`] are consumed by the
//! migration-transaction builder in Phase 4; they are scaffolding until then.

#![allow(dead_code)]

use asterism_core::migration::{
    AccountForAccountSweep, AccountUtxoSet, MigrationPlan, SweepAlgorithm, SweepTransaction,
};
use asterism_core::psbt::UnsignedPsbt;
use asterism_core::{MigrationError, NetworkType};
use bdk_wallet::LocalOutput;
use bitcoin::{Address, Amount, FeeRate};

/// The fixed BIP-48 account index used by the single-account xpub federations.
pub const TREASURY_ACCOUNT_IDX: u32 = 0;

/// A planned single-transaction migration sweep, plus the figures a preview /
/// assertion needs without re-walking the plan.
#[derive(Debug)]
pub struct MigrationSweep {
    /// The underlying library plan. For the single-account treasury case this
    /// always holds exactly one [`SweepTransaction`].
    pub plan: MigrationPlan<UnsignedPsbt>,
    /// The successor-version address the swept funds (minus fee) are routed to.
    /// The plan itself leaves the fee-change output address-less; the consumer
    /// resolves it to this address because `is_fee_final` is `true`.
    pub destination: Address,
    /// Total value of the source UTXOs.
    pub total_in: Amount,
    /// Fee deducted from the treasury (planning estimate; BDK computes the real
    /// fee at PSBT-construction time in Phase 4).
    pub fee: Amount,
    /// Net amount that lands at the destination (`total_in - fee`).
    pub net_out: Amount,
}

impl MigrationSweep {
    /// The single sweep transaction this plan produces.
    pub fn sweep_transaction(&self) -> &SweepTransaction<UnsignedPsbt> {
        self.plan
            .sweep_transactions
            .first()
            .expect("AccountForAccountSweep always yields exactly one transaction")
    }
}

/// Plan a single-transaction federation migration that sweeps `source_utxos`
/// (the current version's UTXOs) to `destination` (the successor version's
/// account-0 address), paying the fee from the swept funds.
///
/// # Errors
///
/// Propagates [`MigrationError`] from the sweep algorithm — notably
/// [`MigrationError::NoUtxos`] when `source_utxos` is empty and
/// [`MigrationError::InsufficientFeeBalance`] when the treasury balance is below
/// the estimated fee.
pub fn plan_migration_sweep(
    source_utxos: Vec<LocalOutput>,
    destination: Address,
    network: NetworkType,
    fee_rate: FeeRate,
) -> Result<MigrationSweep, MigrationError> {
    let total_in = source_utxos
        .iter()
        .map(|u| u.txout.value)
        .fold(Amount::ZERO, |a, b| a + b);

    let account = AccountUtxoSet {
        account_idx: TREASURY_ACCOUNT_IDX,
        utxos: source_utxos,
        destination_address: destination.clone(),
    };

    let plan = AccountForAccountSweep::new(TREASURY_ACCOUNT_IDX).plan(
        std::slice::from_ref(&account),
        network,
        network,
        fee_rate,
    )?;

    let fee = plan.total_fees;
    // Safe: the only account is the fee account, so the algorithm has already
    // validated `treasury balance (== total_in) >= fee` (else InsufficientFeeBalance).
    let net_out = total_in
        .checked_sub(fee)
        .expect("AccountForAccountSweep validated fee <= treasury balance");

    Ok(MigrationSweep {
        plan,
        destination,
        total_in,
        fee,
        net_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_core::migration::SweepOutput;
    use bdk_wallet::KeychainKind;
    use bdk_wallet::chain::ChainPosition;
    use bitcoin::hashes::Hash;
    use bitcoin::{Network, OutPoint, TxOut, Txid};

    const TESTNET: NetworkType = NetworkType::Bitcoin(Network::Testnet);

    fn dest_address() -> Address {
        "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"
            .parse::<Address<_>>()
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap()
    }

    fn dummy_utxo(amount_sat: u64, idx: u32) -> LocalOutput {
        let byte = u8::try_from(idx & 0xff).expect("idx & 0xff fits u8");
        LocalOutput {
            outpoint: OutPoint {
                txid: Txid::from_byte_array([byte; 32]),
                vout: idx,
            },
            txout: TxOut {
                value: Amount::from_sat(amount_sat),
                script_pubkey: dest_address().script_pubkey(),
            },
            keychain: KeychainKind::External,
            is_spent: false,
            derivation_index: idx,
            chain_position: ChainPosition::Unconfirmed {
                first_seen: None,
                last_seen: None,
            },
        }
    }

    fn rate() -> FeeRate {
        FeeRate::from_sat_per_vb_u32(2)
    }

    #[test]
    fn treasury_sweep_is_single_tx_with_one_fee_change_output() {
        let utxos = vec![
            dummy_utxo(500_000, 0),
            dummy_utxo(250_000, 1),
            dummy_utxo(50_000, 2),
        ];
        let sweep = plan_migration_sweep(utxos, dest_address(), TESTNET, rate()).unwrap();

        // Exactly one transaction, finalised onto the new federation.
        assert_eq!(sweep.plan.sweep_transactions.len(), 1);
        let tx = sweep.sweep_transaction();
        assert!(
            tx.is_fee_final,
            "single-account sweep crosses to the new federation"
        );
        assert_eq!(tx.source_utxos.len(), 3, "all UTXOs are spent");

        // The sole output is the treasury's fee-change drain for account 0.
        assert_eq!(tx.outputs.len(), 1);
        match &tx.outputs[0] {
            SweepOutput::FeeChange {
                account_idx,
                amount,
            } => {
                assert_eq!(*account_idx, TREASURY_ACCOUNT_IDX);
                assert_eq!(*amount, sweep.net_out);
            }
            SweepOutput::Customer { .. } => panic!("treasury account must be a FeeChange output"),
        }
    }

    #[test]
    fn value_is_conserved_net_equals_balance_minus_fee() {
        let utxos = vec![dummy_utxo(500_000, 0), dummy_utxo(250_000, 1)];
        let sweep = plan_migration_sweep(utxos, dest_address(), TESTNET, rate()).unwrap();

        assert_eq!(sweep.total_in, Amount::from_sat(750_000));
        assert!(sweep.fee > Amount::ZERO, "a real fee is charged");
        assert_eq!(sweep.net_out + sweep.fee, sweep.total_in);
    }

    #[test]
    fn empty_utxos_is_no_utxos_error() {
        let err = plan_migration_sweep(vec![], dest_address(), TESTNET, rate()).unwrap_err();
        assert!(matches!(err, MigrationError::NoUtxos), "got {err:?}");
    }

    #[test]
    fn dust_below_fee_is_insufficient_fee_balance() {
        // A 1-sat treasury cannot cover any real fee.
        let utxos = vec![dummy_utxo(1, 0)];
        let err = plan_migration_sweep(utxos, dest_address(), TESTNET, rate()).unwrap_err();
        match err {
            MigrationError::InsufficientFeeBalance {
                fee_account_idx,
                available,
                ..
            } => {
                assert_eq!(fee_account_idx, TREASURY_ACCOUNT_IDX);
                assert_eq!(available, Amount::from_sat(1));
            }
            other => panic!("expected InsufficientFeeBalance, got {other:?}"),
        }
    }
}
