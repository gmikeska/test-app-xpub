-- Reorg-reconciliation: bind a migrated-away version's sweep txid to its row so
-- an app-side re-sync can answer "was version V's migration sweep reversed?" from
-- wallet ground-truth.
--
-- xpub models federation *versions* as `federations` rows (grouped by
-- `lineage_id`); this is the analog of test-app-pkcs11's
-- `federation_versions.migration_status` + `migration_sweep_txid`. It is
-- **additive** and orthogonal to the version lifecycle `status`
-- (pending/active/superseded/abandoned) and the migration-record lifecycle
-- (`federation_migrations.status` draft/proposed/enacted/cancelled): those track
-- the governance/version flip, while these two columns track whether the
-- predecessor version's funds have actually landed (and stayed) at the successor.
--
--   migration_status:
--     'not_applicable' : the current/newest version, or one never migrated away
--     'pending'        : migrated away from; its funds still need to be swept
--                        forward (or a prior sweep was reorged out)
--     'in_progress'    : a sweep is executing (reserved; unused in v1 scope)
--     'complete'       : the predecessor's funds have been swept to the successor
--
--   migration_sweep_txid:
--     set together with migration_status='complete' when the sweep is broadcast
--     (see `db::set_migration_complete`); cleared to NULL when
--     `db::reconcile_migration` reverts a version 'complete' -> 'pending' after a
--     reorg evicts the recorded sweep from the wallet's tx graph.

ALTER TABLE federations
    ADD COLUMN IF NOT EXISTS migration_status TEXT NOT NULL DEFAULT 'not_applicable'
        CHECK (migration_status IN (
            'not_applicable',
            'pending',
            'in_progress',
            'complete'
        )),
    ADD COLUMN IF NOT EXISTS migration_sweep_txid TEXT;  -- set on sweep, cleared on reorg-revert
