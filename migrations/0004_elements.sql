-- Elements / Liquid federation support.
--
-- Bitcoin federations and Liquid federations live side-by-side in the same
-- `federations` table; the `network` column distinguishes them:
--
--   Bitcoin:  network IN ('regtest', 'testnet', 'signet', 'bitcoin')
--   Liquid :  network IN ('elementsregtest', 'liquidtestnet', 'liquid')
--
-- The `psbt_b64` column on `transaction_proposals` holds either a Bitcoin PSBT
-- base64 (Bitcoin federations) or an Elements PSET base64 (Liquid federations).
-- The handlers branch on the federation's parsed `FederationKind` (see
-- `db.rs::FederationKind`) to choose between BDK's `Wallet` / `Psbt` and LWK's
-- `Wollet` / `Pset` code paths.

ALTER TABLE federations
    -- 32-byte SLIP-77 master blinding key for Liquid federations. NULL for
    -- Bitcoin. The key is generated server-side at federation creation; the
    -- web app's federations are not confidential between the cosigners on
    -- this server (this app is a developer toy, not a production custody
    -- platform).
    ADD COLUMN IF NOT EXISTS master_blinding_key BYTEA NULL
        CHECK (
            master_blinding_key IS NULL
            OR octet_length(master_blinding_key) = 32
        ),
    -- LWK has no `ChangeSet`-style persistence. We track the next index to
    -- reveal on each keychain in the federations row so wallets stay in sync
    -- across restarts. BDK federations ignore these (they round-trip through
    -- `bdk_changeset`).
    ADD COLUMN IF NOT EXISTS next_external_index INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS next_internal_index INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS federations_network_idx ON federations (network);
