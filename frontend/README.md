# Aegis Frontend

**Live demo:** https://frontend-five-gamma-10.vercel.app

A single self-contained `index.html` — no build step. Open it directly or serve it:

```bash
cd frontend
python3 -m http.server 8080   # then open http://localhost:8080
```

## What it shows

The demo walks the judge through the exact on-chain flow in `scripts/e2e-demo.sh`:

1. **Proof of Reserves** — issuer proves `sum(reserves) ≥ supply` against a published
   Poseidon commitment. Balances stay redacted; only the verdict is learned on-chain.
2. **Investor Eligibility** — investor proves KYC ≥ 2, allowlisted jurisdiction, and
   accreditation as **one boolean + a nullifier**. Identity/DOB/country never leave the wallet.
3. **The Gate** — `rwa_gate.authorize_receive` opens only when reserves are fresh **and**
   the receiver is eligible, then the nullifier is spent so the proof can't be replayed.

## Wiring to a live testnet deployment

The shipped UI is a faithful simulation so the video runs without a funded wallet. To make
it call real contracts, replace the `mock*` interactions in the inline `<script>` with
`@stellar/stellar-sdk` contract invokes, reading addresses from `../build/deploy.testnet.json`.
The proof JSON is produced by `@aegis/prover` (`prover/src/prove-*.js`) in the exact byte
layout the Soroban verifier expects (`prover/src/soroban-format.js`). See `docs/UPGRADE.md`
section "Wiring the frontend to live contracts".
