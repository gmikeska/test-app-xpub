-- Dual-chain federations: a federation is created from members only (no
-- up-front Bitcoin/Liquid choice). The `descriptor` column holds the Bitcoin
-- `wsh(sortedmulti(...))`; when every cosigner device is a Jade (the only
-- consumer device that signs Liquid), we ALSO materialize the Elements
-- confidential descriptor `ct(slip77(mbk), elwsh(sortedmulti(...)))` and store
-- it here. Its presence is the flag that the federation is Elements-capable
-- and the top-of-page Bitcoin<->Elements toggle should appear.
--
-- NULL = Bitcoin-only federation (a non-Jade cosigner is present).
ALTER TABLE federations
    ADD COLUMN IF NOT EXISTS elements_descriptor TEXT NULL;
