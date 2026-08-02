-- Reorg-reconciliation, phase 3 (funds-preserving re-completion): allow a
-- fourth proposal `kind` = 'resweep'.
--
-- Background. A completed migration whose sweep S is *censorship-reorged* —
-- its confirmation stripped while the funding tx D stays confirmed — is
-- reverted by 0008's confirmation-loss predicate to `migration_status =
-- 'pending'` on the (now superseded) base version, with the funds preserved in
-- D. The version flip itself is NOT undone (v0 stays superseded, v1 active);
-- only the reorg-reconciliation column reverts. To finish the migration the
-- funds must be swept forward again, from the superseded base to the active
-- successor.
--
-- The ordinary migration/relay drain cannot build that re-sweep: after the
-- reorg the old sweep S sits unconfirmed-in-mempool spending D's output, so BDK
-- marks D spent-by-mempool and `drain_wallet()` selects nothing. The re-sweep
-- (`kind = 'resweep'`) instead force-respends D's stranded output and signals
-- BIP-125 RBF at a higher fee, so the new sweep S' displaces the old S in the
-- mempool. On broadcast it re-marks the base version `complete` (recording S'
-- as the sweep) — the re-completion counterpart to `set_migration_complete`.
--
-- A resweep reuses the whole propose → sign → finalize → broadcast machinery
-- and, like a migration, links back to its `federation_migrations` record via
-- `migration_id` (the already-`enacted` migration it re-completes). Unlike a
-- migration it triggers NO version flip on broadcast — the flip already
-- happened; broadcasting only re-marks `migration_status` complete.

ALTER TABLE transaction_proposals
    DROP CONSTRAINT IF EXISTS transaction_proposals_kind_check;

ALTER TABLE transaction_proposals
    ADD CONSTRAINT transaction_proposals_kind_check
        CHECK (kind IN ('send', 'migration', 'relay', 'resweep'));
