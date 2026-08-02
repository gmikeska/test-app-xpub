# test-app-xpub — TODO

## Verification

- [ ] **Full end-to-end pass on both `esplora`, `waterfalls` and raw `RPC` chain backends.** Re-run the
      complete dual-chain flow against each backend and confirm parity with the electrum
      run already proven (2026-08-02). For **each** backend (esplora, RPC), on both
      Bitcoin and Elements:
  - Onboard both Jades → create a dual-chain vault (both descriptors).
  - Fund each side; confirm balances + per-address detail render.
  - **Send** (explicit amount) and **Send → Max** (drain): proposal shows real
    Amount / Fee / Inputs → sign with both Jades → broadcast → confirm receipt on the node.
  - **Migration**, single-chain-funded: sweep + version flip.
  - **Migration**, both-chains-funded: two sweeps against one migration, sign each on its
    chain view, broadcast both, verify the version flips exactly once and both chains'
    coin lands on the successor.
  - Confirm the `chain=bitcoin` / `chain=elements` proposal filter holds on each view.
  - Watch for backend-specific gaps like the electrum one we hit: the Trezor sign-data
    path fetches input prev-txs over bitcoind RPC — make sure whichever backend is wired
    actually serves them (or the signer is Jade, which skips it).
