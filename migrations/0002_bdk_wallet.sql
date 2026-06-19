-- BDK wallet state for each federation.
--
-- We attach one `bdk_wallet::Wallet` to every federation: it tracks revealed
-- addresses (external + change keychains), UTXOs, and the local chain tip.
-- The wallet's full `ChangeSet` is persisted into `bdk_changeset` (JSONB) so
-- restarting the web app doesn't lose sync state. `chain_tip_height` is a
-- denormalised convenience for the federation detail page header.

ALTER TABLE federations
    ADD COLUMN IF NOT EXISTS bdk_changeset    JSONB,
    ADD COLUMN IF NOT EXISTS chain_tip_height INTEGER;
