-- Per-proposal chain tag. A dual-chain federation can carry both Bitcoin
-- (PSBT) and Elements (PSET) proposals — migration sweeps and, later, ordinary
-- sends. The sign/finalize/broadcast handlers key their PSBT-vs-PSET logic on
-- this column rather than the federation's network (which is always Bitcoin for
-- a dual-chain vault).
--
-- `bitcoin` (default, back-compat for every existing row) | `elements`.
ALTER TABLE transaction_proposals
    ADD COLUMN IF NOT EXISTS chain TEXT NOT NULL DEFAULT 'bitcoin';
