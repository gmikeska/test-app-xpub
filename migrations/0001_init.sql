-- test-app-xpub schema (emvault_xpub database).
--
-- The web app's domain model:
--   users               -- login identities (email + Argon2id hash)
--   signers             -- onboarded hardware-wallet xpubs (one+ per user)
--   federations         -- emvault Federation snapshots
--   federation_members  -- which user/signer participates in which federation
--
-- "First login" for a given user is defined as: the user has zero rows in
-- `signers`. The web app uses this to decide whether to send them to
-- /onboard or /home.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ----------------------------------------------------------------------------
-- users
-- ----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS users_email_idx ON users (lower(email));

-- ----------------------------------------------------------------------------
-- signers
-- ----------------------------------------------------------------------------
--
-- Captured from the user's hardware wallet via the browser. Each row mirrors
-- the inputs to `emvault::xpub::ExternalSigner::from_descriptor_key(...)`:
-- the literal descriptor-key string the device exported, plus the parsed
-- (fingerprint, derivation_path, xpub) triple so the homepage can render
-- them without re-parsing.

CREATE TABLE IF NOT EXISTS signers (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label           TEXT,
    descriptor_key  TEXT        NOT NULL,
    xpub            TEXT        NOT NULL,
    fingerprint     TEXT        NOT NULL,
    derivation_path TEXT        NOT NULL,
    device_type     TEXT        NOT NULL,
    network         TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, fingerprint)
);

CREATE INDEX IF NOT EXISTS signers_user_idx ON signers (user_id);

-- ----------------------------------------------------------------------------
-- federations
-- ----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS federations (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    label         TEXT        NOT NULL,
    threshold     INT         NOT NULL CHECK (threshold > 0),
    total_signers INT         NOT NULL CHECK (total_signers >= threshold),
    network       TEXT        NOT NULL,
    descriptor    TEXT        NOT NULL,
    snapshot_json JSONB       NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ----------------------------------------------------------------------------
-- federation_members
-- ----------------------------------------------------------------------------
--
-- Many-to-many join between users and federations. A user contributes
-- exactly one signer to a given federation (the column is nullable only to
-- accommodate post-creation rotation flows that haven't been built yet).

CREATE TABLE IF NOT EXISTS federation_members (
    federation_id UUID        NOT NULL REFERENCES federations(id) ON DELETE CASCADE,
    user_id       UUID        NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
    signer_id     UUID                 REFERENCES signers(id)     ON DELETE SET NULL,
    role          TEXT        NOT NULL DEFAULT 'trustee',
    joined_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (federation_id, user_id)
);

CREATE INDEX IF NOT EXISTS federation_members_user_idx ON federation_members (user_id);
