-- Federation versioning: turn standalone `federations` rows into a **lineage**
-- of immutable **versions** (v0 → v1 → …). A migration mints version N+1; the
-- newest `active` version is "current". Removed signers stay members of the
-- historic versions they belonged to (so they can relay late inflows); new
-- signers are members of the current version only. See
-- `emerald_multisignature/xpub_federation_migration.md` §6.

ALTER TABLE federations
    ADD COLUMN IF NOT EXISTS lineage_id     UUID,
    ADD COLUMN IF NOT EXISTS version_index  INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS predecessor_id UUID REFERENCES federations(id),
    ADD COLUMN IF NOT EXISTS status         TEXT    NOT NULL DEFAULT 'active';

-- Backfill any pre-versioning rows into single-version lineages: each existing
-- federation becomes v0 (`active`) of a lineage identified by its own id.
UPDATE federations
   SET lineage_id = id
 WHERE lineage_id IS NULL;

-- lineage_id has no column-level default (it equals the row's own id), so it is
-- only enforced NOT NULL after the backfill above. New rows set it explicitly
-- (see `db::insert_federation_with_members`).
ALTER TABLE federations
    ALTER COLUMN lineage_id SET NOT NULL;

ALTER TABLE federations
    ADD CONSTRAINT federations_status_check
        CHECK (status IN ('pending', 'active', 'superseded', 'abandoned'));

-- Exactly one composition per (lineage, version_index).
ALTER TABLE federations
    ADD CONSTRAINT federations_lineage_version_unique UNIQUE (lineage_id, version_index);

-- At most one `active` version per lineage (the "current" one).
CREATE UNIQUE INDEX IF NOT EXISTS federations_one_active_per_lineage
    ON federations (lineage_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS federations_lineage_idx ON federations (lineage_id);
