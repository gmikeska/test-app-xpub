-- Taproot support: record each federation version's script type and, for
-- xpub-NUMS taproot federations, the per-federation NUMS chain code (recovery
-- material — it also lives inside the descriptor's NUMS xpub).
ALTER TABLE federations
    ADD COLUMN script_type TEXT NOT NULL DEFAULT 'wsh',
    ADD COLUMN nums_chaincode BYTEA;
