-- Federation migrations: the version-change *record* (roster change → version
-- N+1). The signed sweep that actually moves the funds lives in
-- `transaction_proposals` with `kind = 'migration'` (see 0006); this table is
-- the governance/record side. "Relay" sweeps are NOT migrations — they reuse
-- the proposal machinery with `kind = 'relay'` and do not get a row here.
-- See `emerald_multisignature/xpub_federation_migration.md` §3, §5.

CREATE TABLE IF NOT EXISTS federation_migrations (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The lineage being migrated.
    lineage_id        UUID        NOT NULL,
    -- The current version this migration amends.
    base_version_id   UUID        NOT NULL REFERENCES federations(id) ON DELETE CASCADE,
    -- The pending successor version this migration mints. NULL until the
    -- pending version row is created (Phase 3); SET NULL if it is abandoned.
    target_version_id UUID                 REFERENCES federations(id) ON DELETE SET NULL,
    -- The member who started the migration.
    proposed_by       UUID        NOT NULL REFERENCES users(id),
    -- Threshold (m) chosen for the next version.
    next_threshold    INTEGER     NOT NULL CHECK (next_threshold > 0),
    -- Lifecycle:
    --   'draft'     : roster being edited, no pending version yet
    --   'proposed'  : pending version minted, migration transaction in flight
    --   'enacted'   : migration transaction broadcast; version flip applied
    --   'cancelled' : abandoned before broadcast
    status            TEXT        NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'proposed', 'enacted', 'cancelled')),
    description       TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS federation_migrations_lineage_idx
    ON federation_migrations (lineage_id, created_at DESC);

-- At most one in-flight (draft/proposed) migration per lineage.
CREATE UNIQUE INDEX IF NOT EXISTS federation_migrations_one_inflight_per_lineage
    ON federation_migrations (lineage_id)
    WHERE status IN ('draft', 'proposed');

-- The roster diff for a migration: per prospective member, the action taken.
-- `keep` rows make the next version's full roster explicit; `add`/`remove`
-- describe the change relative to the base version.
CREATE TABLE IF NOT EXISTS migration_changes (
    migration_id UUID NOT NULL REFERENCES federation_migrations(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id),
    -- The signer the member contributes to the NEXT version (for add/keep).
    -- Nullable for `remove`, or for an add whose invitee hasn't onboarded yet.
    signer_id    UUID          REFERENCES signers(id) ON DELETE SET NULL,
    action       TEXT NOT NULL CHECK (action IN ('add', 'remove', 'keep')),
    role         TEXT NOT NULL DEFAULT 'trustee',
    PRIMARY KEY (migration_id, user_id)
);

CREATE INDEX IF NOT EXISTS migration_changes_migration_idx
    ON migration_changes (migration_id);
