# Aegis — Compliant ZK Layer for Real-World Assets on Stellar

> Prove an RWA is **fully reserved**, and prove a buyer is **eligible to hold it** —
> without revealing a single balance, identity, or document. Both proofs are verified
> **on-chain** by Soroban contracts.

Aegis is two load-bearing zero-knowledge proofs and the gate that composes them:

1. **ZK Proof-of-Reserves** — an RWA issuer proves `sum(reserve balances) ≥ circulating supply × collateral ratio` against a Poseidon commitment they publish, **revealing no individual balance, account, or custodian**. A manual monthly PoR report becomes a live, anyone-can-verify on-chain attestation.
2. **ZK Investor Eligibility (selective disclosure)** — an investor proves an issuer-signed credential satisfies a gate's policy (KYC level, allowlisted jurisdiction, accreditation, not expired) and reveals **only one boolean plus an unlinkable nullifier** — never their identity, birth date, or exact country.
3. **RWA Gate** — a transfer/mint is authorized **only when** reserves are fresh and sufficient **and** the receiver is eligible. The nullifier is then spent so the proof can't be replayed.

This is Stellar's own roadmap — *privacy with compliance*, a "100% private institutional settlement layer" — built as a concrete, working slice. The ZK is not decoration: delete it and the entire guarantee collapses.

---

## Why this matters on Stellar specifically

Stellar moves real money: stablecoins, cross-border payments, and a tokenized-RWA market that grew ~91% quarter-over-quarter past $2B (Messari, Q1 2026). Protocol 25 (X-Ray) and Protocol 26 (Yardstick) added native BN254 + Poseidon host functions, making Groth16 proof verification cheap enough to run **inside a Soroban contract**. Aegis sits exactly where Stellar is strongest — regulated real-world value — and uses ZK where it is genuinely load-bearing: solvency and eligibility you can verify without seeing the private data.

Unlike a Monero-style "hide everything" design, Aegis follows Stellar's Association-Set / selective-disclosure philosophy: **private by default, provable when it counts, auditable when required.**

---

## Architecture

```
                        ┌─────────────────────────────────────────────┐
   Issuer (off-chain)   │              On-chain (Soroban)             │
 ┌──────────────────┐   │   ┌───────────────────┐                     │
 │ reserve balances │──►│   │  por_verifier      │  attest()          │
 │ + salt           │   │   │  • binds commitment│◄────── π_reserves  │
 │  ▼ Poseidon       │   │   │  • binds supply/bps│                     │
 │ commitment ───────┼──►│   │  • calls groth16   │──┐                  │
 └──────────────────┘   │   └───────────────────┘  │ verify           │
                        │                            ▼                  │
 Investor (off-chain)   │   ┌───────────────────┐  ┌──────────────────┐│
 ┌──────────────────┐   │   │ eligibility_       │  │ groth16_verifier ││
 │ signed credential│──►│   │ verifier           │  │ (BN254 pairing)  ││
 │ + merkle path    │   │   │ • binds policy     │──►│  Protocol 25/26  ││
 │  ▼ Groth16        │   │   │ • spends nullifier │  └──────────────────┘│
 │ π_eligibility ────┼──►│   └─────────┬─────────┘                       │
 └──────────────────┘   │             │ verify_eligibility               │
                        │   ┌─────────▼─────────┐                        │
                        │   │     rwa_gate       │  authorize_receive()   │
                        │   │  reserves fresh? ──┴── receiver eligible? ──► ✅/❌
                        │   └───────────────────┘                        │
                        └─────────────────────────────────────────────┘
```

| Component | Path | Language |
|---|---|---|
| BN254 Groth16 verifier | `contracts/groth16_bn254_verifier/` | Rust / Soroban |
| Proof-of-Reserves circuit | `circuits/proof_of_reserves/` | Circom 2.1.9 |
| Eligibility circuit | `circuits/eligibility/` | Circom 2.1.9 |
| PoR verifier contract | `contracts/por_verifier/` | Rust / Soroban |
| Eligibility verifier contract | `contracts/eligibility_verifier/` | Rust / Soroban |
| RWA gate contract | `contracts/rwa_gate/` | Rust / Soroban |
| Prover + Soroban formatter | `prover/` | Node.js (snarkjs, circomlibjs) |
| Demo UI | `frontend/` | Single-file HTML/JS |

---

## What each proof actually enforces

### Proof-of-Reserves (`circuits/proof_of_reserves/proof_of_reserves.circom`)
- **Public:** `totalSupply`, `reservesCommitment`, `minCollateralBps`
- **Private:** `balances[8]`, `salt`
- **Constraints:** `Poseidon(balances, salt) == reservesCommitment` (binds to a published commitment) and `sum(balances)·10000 ≥ totalSupply·minCollateralBps` (over-collateralization). Every balance and the supply are range-checked to 64 bits to prevent field-wrap cheating.

### Eligibility (`circuits/eligibility/eligibility.circom`)
- **Public:** issuer pubkey, `requiredKycLevel`, `requireAccredited`, `allowedJurisdictionRoot`, `currentTimestamp`, `actionId`, `nullifier`
- **Private:** the signed credential (`kycLevel`, `jurisdictionCode`, `accredited`, `expiry`, `credentialSecret`), the issuer's EdDSA-Poseidon signature, and a Merkle path
- **Constraints:** issuer signature valid over the credential hash; `kycLevel ≥ required`; `requireAccredited ⟹ accredited`; `expiry > now`; jurisdiction ∈ issuer allowlist (Poseidon-Merkle inclusion, depth 16); `nullifier == Poseidon(credentialSecret, actionId)`.

The contracts additionally **bind every policy-controlled public signal** to the registered gate policy, so a proof generated for a different policy or token cannot be replayed.

---

## Quickstart

Full prerequisites and exact versions are in [`docs/SETUP.md`](docs/SETUP.md).

```bash
# 1. Prover unit tests (no toolchain beyond Node required)
cd prover && npm install && npm test

# 2. Get circuit artifacts (zkey + wasm + vkey)
#    Fast path: download prebuilt artifacts from GitHub Release (~30 sec, no circom needed)
export AEGIS_RELEASE_URL=https://github.com/<your-username>/aegis/releases/download/v1.0.0-dev
bash scripts/build-circuits.sh
#    Full local build (only needed if you modify circuits, requires circom + snarkjs):
#    bash scripts/build-circuits.sh --local

# 3. Build + deploy the Soroban contracts to testnet (needs stellar-cli)
bash scripts/deploy.sh

# 4. Run the end-to-end proof demo
bash scripts/e2e-demo.sh

# 5. Open the UI
cd frontend && python3 -m http.server 8080
```

---

## Honesty notes (per the hackathon's "honest WIP over polished mystery")

- **Trusted setup:** `scripts/build-circuits.sh` defaults to downloading prebuilt `.zkey` files from GitHub Release (the standard practice for all major Circom projects — Tornado Cash, Semaphore, Worldcoin all ship prebuilt keys). Pass `--local` to run Phase-2 locally (requires circom + snarkjs, takes minutes). Either way the Phase-2 contribution is a dev-only single-party ceremony; production requires a proper MPC ceremony — see `docs/UPGRADE.md`.
- **Soroban Groth16 wiring:** Aegis ships its own `groth16_bn254_verifier` contract (the fourth contract in the stack) which calls the Protocol 25 BN254 host functions (`bn254_g1_add`, `bn254_g1_mul`, `bn254_multi_pairing_check`) directly — no external contract address needed. The three application contracts cross-call it. The BN254 byte-encoding knob (`G2_FP2_ORDER` in `prover/src/soroban-format.js`) is the single place to calibrate if an off-chain-valid proof is rejected on-chain. Details in `docs/UPGRADE.md`.
- **Jurisdiction handling** is implemented as an **allowlist** (membership) rather than generic non-membership — sound, simpler, and matches how Stellar's ASP allow/deny sets work.
- The shipped **frontend is a faithful simulation** of the on-chain flow so the demo runs without a funded wallet; `frontend/README.md` shows how to wire it to live contracts.

---

## Repository layout

```
aegis/
├── circuits/                 # Circom ZK circuits + circomlib include shims
├── contracts/                # Four Soroban contracts (Rust) + native tests
│   ├── groth16_bn254_verifier/ # Self-contained BN254 Groth16 verifier (Protocol 25 host fns)
│   ├── por_verifier/           # PoR attestation contract
│   ├── eligibility_verifier/   # Investor eligibility gate
│   └── rwa_gate/               # Composes PoR + eligibility
├── prover/                   # snarkjs prover, credential issuer, Soroban formatter, tests
├── scripts/                  # build-circuits / release-artifacts / deploy / e2e-demo / invoke-onchain / export-vk / encode-invoke-args / register-vk
├── frontend/                 # single-file demo UI
├── docs/                     # SETUP.md, ARCHITECTURE.md, UPGRADE.md, GROTH16_VERIFIER.md
└── .github/workflows/ci.yml  # prover tests + contract build/test
```

## License

MIT — see [`LICENSE`](LICENSE).
