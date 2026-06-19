-- Candidate-send proposals: outgoing transactions in flight against a federation.
--
-- Lifecycle (`status` column):
--   'proposed'  : created, zero signatures yet
--   'signing'   : at least one cosigner partial signature has been merged
--   'finalized' : threshold met, PSBT finalised + raw tx extracted, NOT yet broadcast
--   'broadcast' : sendrawtransaction succeeded; txid persisted
--   'cancelled' : the proposer cancelled before broadcast
--
-- Rejections (see `transaction_rejections`) are advisory: they are recorded
-- for audit but never auto-flip `status`. The proposer is the only role that
-- can move a proposal into `cancelled`.

CREATE TABLE IF NOT EXISTS transaction_proposals (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    federation_id       UUID        NOT NULL REFERENCES federations(id) ON DELETE CASCADE,
    proposed_by         UUID        NOT NULL REFERENCES users(id),
    label               TEXT,
    status              TEXT        NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed', 'signing', 'finalized', 'broadcast', 'cancelled')),
    -- Canonical PSBT (base64). Mutates as cosigner partial sigs are merged in.
    psbt_b64            TEXT        NOT NULL,
    -- Structural view of the unsigned tx: outputs, total, fee, fee_rate. Lets
    -- the UI render the proposal without re-deserialising the PSBT.
    proposal_json       JSONB       NOT NULL,
    -- BDK's coin-selection result: selected UTXOs (with keychain/index) and
    -- the output split (recipient/change/fee). Rendered in the collapsible
    -- "Coin selection" panel on the proposal page.
    coin_selection_json JSONB       NOT NULL,
    -- Populated when status transitions to 'finalized' / 'broadcast'.
    finalized_tx_hex    TEXT,
    txid                TEXT,
    broadcast_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS proposals_fed_status_idx
    ON transaction_proposals (federation_id, status);

CREATE INDEX IF NOT EXISTS proposals_fed_created_idx
    ON transaction_proposals (federation_id, created_at DESC);

-- One row per cosigner contribution. Append-only; a re-sign by the same
-- cosigner becomes an idempotent no-op at the handler layer (ON CONFLICT
-- DO NOTHING).
CREATE TABLE IF NOT EXISTS transaction_signatures (
    proposal_id      UUID        NOT NULL REFERENCES transaction_proposals(id) ON DELETE CASCADE,
    signer_id        UUID        NOT NULL REFERENCES signers(id),
    user_id          UUID        NOT NULL REFERENCES users(id),
    -- The merged partial PSBT (base64) the cosigner contributed.
    partial_psbt_b64 TEXT        NOT NULL,
    signed_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (proposal_id, signer_id)
);

CREATE INDEX IF NOT EXISTS signatures_proposal_idx
    ON transaction_signatures (proposal_id);

-- Advisory only. A rejection is recorded for audit and rendered on the
-- proposal page, but the proposal continues to accept signatures until it
-- is cancelled by the proposer or finalised + broadcast.
CREATE TABLE IF NOT EXISTS transaction_rejections (
    proposal_id  UUID        NOT NULL REFERENCES transaction_proposals(id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id),
    reason       TEXT,
    rejected_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (proposal_id, user_id)
);

CREATE INDEX IF NOT EXISTS rejections_proposal_idx
    ON transaction_rejections (proposal_id);
