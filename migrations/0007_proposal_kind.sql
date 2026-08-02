-- Extend transaction_proposals to carry migration/relay sweeps alongside
-- ordinary sends. A migration transaction and a relay sweep both reuse the
-- existing propose → sign → finalize → broadcast machinery; `kind`
-- distinguishes them and `migration_id` links a migration/relay back to its
-- version-change record (NULL for ordinary sends and for relays, which are not
-- tied to a version change). See
-- `emerald_multisignature/xpub_federation_migration.md` §3, §5.3, §6.3.

ALTER TABLE transaction_proposals
    ADD COLUMN IF NOT EXISTS kind         TEXT NOT NULL DEFAULT 'send',
    ADD COLUMN IF NOT EXISTS migration_id UUID REFERENCES federation_migrations(id) ON DELETE SET NULL;

ALTER TABLE transaction_proposals
    ADD CONSTRAINT transaction_proposals_kind_check
        CHECK (kind IN ('send', 'migration', 'relay'));

CREATE INDEX IF NOT EXISTS proposals_migration_idx
    ON transaction_proposals (migration_id)
    WHERE migration_id IS NOT NULL;
