# Aegis — Compliant ZK Layer for Real-World Assets on Stellar

> Prove an RWA is **fully reserved**, and prove a buyer is **eligible to hold it** —
> without revealing a single balance, identity, or document. Both proofs are verified
> **on-chain** by Soroban contracts.

Aegis is two load-bearing zero-knowledge proofs and the gate that composes them, verified by four Soroban contracts:

1. **ZK Proof-of-Reserves** — an RWA issuer proves `sum(reserve balances) ≥ circulating supply × collateral ratio` against a Poseidon commitment they publish, **revealing no individual balance, account, or custodian**. A manual monthly PoR report becomes a live, anyone-can-verify on-chain attestation.
2. **ZK Investor Eligibility (selective disclosure)** — an investor proves an issuer-signed credential satisfies a gate's policy (KYC level, allowlisted jurisdiction, accreditation, not expired) and reveals **only one boolean plus an unlinkable nullifier** — never their identity, birth date, or exact country.
3. **RWA Gate** — a transfer/mint is authorized **only when** reserves are fresh and sufficient **and** the receiver is eligible. The nullifier is then spent so the proof can't be replayed.
4. **Self-contained BN254 Groth16 Verifier** — a pure cryptographic library contract that verifies both proofs using Protocol 25 host functions (`bn254_g1_add`, `bn254_g1_mul`, `bn254_multi_pairing_check`). No external dependencies.

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
  │ commitment ───────┼──►│   │  • calls verify    │──┐                  │
  └──────────────────┘   │   └───────────────────┘  │                  │
                         │                            ▼                  │
  Investor (off-chain)   │   ┌───────────────────┐  ┌──────────────────┐│
  ┌──────────────────┐   │   │ eligibility_       │  │ groth16_bn254_   ││
  │ signed credential│──►│   │ verifier           │  │ verifier         ││
  │ + merkle path    │   │   │ • binds policy     │──►│  • BN254 pairing ││
  │  ▼ Groth16        │   │   │ • spends nullifier │  │  • Protocol 25   ││
  │ π_eligibility ────┼──►│   └─────────┬─────────┘   │    host fns      ││
  └──────────────────┘   │             │               │  • Self-contained││
                         │             ▼               └──────────────────┘│
                         │   ┌─────────┴─────────┐                        │
                         │   │     rwa_gate       │  authorize_receive()   │
                         │   │  reserves fresh? ──┴── receiver eligible? ──► ✅/❌
                         │   └───────────────────┘                        │
                         └─────────────────────────────────────────────┘
```

| Component | Path | Language |
|---|---|---|
| Proof-of-Reserves circuit | `circuits/proof_of_reserves/` | Circom 2.1.9 |
| Eligibility circuit | `circuits/eligibility/` | Circom 2.1.9 |
| **Groth16 BN254 verifier contract** | `contracts/groth16_bn254_verifier/` | Rust / Soroban |
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

**Live demo UI:** https://frontend-five-gamma-10.vercel.app

Full prerequisites and exact versions are in [`docs/SETUP.md`](docs/SETUP.md).

```bash
# 1. Prover unit tests (no toolchain beyond Node required)
cd prover && npm install && npm test

# 2. Compile circuits + Groth16 trusted setup (needs circom + snarkjs)
cd .. && bash scripts/build-circuits.sh

# 3. Build + deploy the Soroban contracts to testnet (needs stellar-cli)
bash scripts/deploy.sh

# 4. Run the end-to-end proof demo
bash scripts/e2e-demo.sh

# 5. Open the UI
cd frontend && python3 -m http.server 8080
```

---

## Honesty notes (per the hackathon's "honest WIP over polished mystery")

- **Trusted setup:** `scripts/build-circuits.sh` runs a *development* Powers-of-Tau ceremony. Production requires a real multi-party ceremony — see `docs/UPGRADE.md`.
- **Self-contained verifier:** The `groth16_bn254_verifier` contract uses Protocol 25 host functions (`bn254_g1_add`, `bn254_g1_mul`, `bn254_multi_pairing_check`) via soroban-sdk 25.x. No external dependencies — the entire stack deploys from one repo.
- **BN254 byte-encoding:** The `G2_FP2_ORDER` constant in `prover/src/soroban-format.js` is the single calibration knob. If an off-chain-valid proof is rejected on-chain, flip it (`c1c0` ↔ `c0c1`). Details in `docs/UPGRADE.md`.
- **Jurisdiction handling** is implemented as an **allowlist** (membership) rather than generic non-membership — sound, simpler, and matches how Stellar's ASP allow/deny sets work.
- The shipped **frontend is a faithful simulation** of the on-chain flow so the demo runs without a funded wallet; `frontend/README.md` shows how to wire it to live contracts.

---

## Contract interfaces

### `groth16_bn254_verifier` — Self-contained BN254 Groth16 verifier
| Method | Purpose |
|---|---|
| `init(admin)` | One-time init; sets admin |
| `register_vk(vk_id, alpha, beta, gamma, delta, ic)` | Admin registers a verification key for a circuit (vk_id 0=PoR, 1=Eligibility) |
| `vk(vk_id) -> Option<VerificationKey>` | Read a registered VK |
| `verify(vk_id, proof_a, proof_b, proof_c, public_inputs) -> bool` | Verifies a Groth16 proof using BN254 pairing check |

### `por_verifier` — Proof-of-Reserves attestation
| Method | Purpose |
|---|---|
| `init(admin, verifier)` | One-time init; `verifier` is the deployed groth16_verifier |
| `set_policy(token, issuer, reserves_commitment, min_collateral_bps, vk_id)` | Issuer registers/updates PoR policy for a token (`issuer.require_auth()`) |
| `attest(token, claimed_supply, proof, signals)` | Binds public signals to policy, verifies SNARK via cross-contract call, writes timestamped `Attestation` |
| `last_attestation(token) -> Option<Attestation>` | Read latest verified attestation |
| `policy(token) -> Option<ReservePolicy>` | Read policy |

### `eligibility_verifier` — Selective-disclosure gate
| Method | Purpose |
|---|---|
| `init(admin, verifier)` | One-time init |
| `set_gate(admin, gate_id, policy)` | Register eligibility policy for a gate (admin auth) |
| `verify_eligibility(gate_id, proof, signals)` | Bind policy signals → check timestamp skew → check nullifier unused → verify SNARK → consume nullifier |
| `is_nullifier_used(gate_id, nullifier) -> bool` | Query nullifier status |
| `gate(gate_id) -> Option<GatePolicy>` | Read gate policy |

### `rwa_gate` — Composing gate
| Method | Purpose |
|---|---|
| `init(admin)` | One-time init |
| `configure(admin, config)` | Configure gate for a token (admin auth) |
| `check_reserves(token) -> bool` | Check reserves attestation is fresh within `max_reserve_age_secs` |
| `authorize_receive(token, receiver, eligibility_proof, eligibility_signals) -> BytesN<32>` | **Atomic**: reserves fresh AND eligibility passes → emit event + return consumed nullifier. Either failure rolls back the whole tx. |
| `config(token) -> Option<GateConfig>` | Read config |

---

## Prover toolchain

| File | Purpose |
|---|---|
| `src/field.js` | Poseidon / EdDSA / field arithmetic (circomlibjs), `FIELD_MODULUS`, `mod()` normalization |
| `src/merkle.js` | `PoseidonMerkleTree`, `buildAllowlistTree` — jurisdiction allowlist tree + membership proofs |
| `src/soroban-format.js` | Converts snarkjs proof/VK to contract-expected big-endian bytes; `G2_FP2_ORDER` is the single calibration knob |
| `src/issue-credential.js` | Issuer signs investor credential with EdDSA-Poseidon |
| `src/build-allowlist.js` | Builds jurisdiction Merkle tree from country codes |
| `src/prove-reserves.js` | Generates reserves proof, outputs `reservesCommitment` + Soroban-formatted proof |
| `src/prove-eligibility.js` | Generates eligibility proof, outputs `nullifier` + Soroban-formatted proof |
| `test/core.test.js` | 9 offline tests: Poseidon determinism, field normalization, Merkle membership, credential signing, commitment/nullifier derivation |

---

## Competition alignment (Stellar Hacks: Real-World ZK)

| Criteria | Aegis approach |
|---|---|
| **ZK is load-bearing** | Delete either proof and the entire guarantee collapses; both verified on-chain |
| **Real-world real money** | Targets RWA + stablecoin settlement — SDF's golden use case |
| **Privacy + compliance** | Selective disclosure + jurisdiction allowlist + nullifier — matches SDF "private institutional settlement" roadmap, not full anonymity |
| **Blue-ocean differentiation** | Avoids saturated directions: anonymous voting, bare zkKYC, bare private payments |
| **Runnable / verifiable** | 9 offline tests + native contract tests + e2e + on-chain invoke scripts, CI covered |
| **Honest WIP** | Dev trusted setup, byte-encoding calibration, frontend simulation — all transparent |

---

## Self-check checklist

- [ ] `cd prover && npm install && npm test` → 9 tests pass
- [ ] `circom --version` ≥ 2.1.9, `stellar --version` ≥ 22
- [ ] `bash scripts/build-circuits.sh` produces `build/*_final.zkey`, `build/*_vk_soroban.json`
- [ ] `bash scripts/e2e-demo.sh` generates `build/e2e/{reserves_proof,eligibility_proof,credential,allowlist}.json`
- [ ] `cd contracts && cargo test --workspace` → all native tests pass
- [ ] `cd contracts && stellar contract build` → four `.wasm` files
- [ ] `bash scripts/deploy.sh` writes `build/deploy.testnet.json`
- [ ] `bash scripts/invoke-onchain.sh` → first `authorize_receive` succeeds, second rejected (nullifier spent)
- [ ] Frontend `python3 -m http.server 8080` opens correctly

---

## Troubleshooting

| Symptom | Cause / Fix |
|---|---|
| `circom: command not found` | Install circom ≥ 2.1.9 from source |
| `Cannot find module 'circomlib'` | Run `npm install circomlib@2.0.5` at repo root |
| `cargo build` missing BN254 methods | Ensure `soroban-sdk` is 25.x with `hazmat` feature in `contracts/groth16_bn254_verifier/Cargo.toml` |
| Off-chain proof valid but on-chain rejected | Flip `G2_FP2_ORDER` in `prover/src/soroban-format.js` (`c1c0` ↔ `c0c1`) |
| friendbot funding fails | Testnet rate limit — retry or `stellar keys fund <id> --network testnet` |
| `Error: VkNotFound` | VK not registered — run `node scripts/register-vk.mjs` after building circuits |

---

## Testnet deployment

All four contracts are deployed on Stellar testnet:

| Contract | Address |
|---|---|
| `groth16_bn254_verifier` | `CBCZQMNXATGWCKTZPEXYFA7MO4R7EULQP4LRHWBCORPGONMMWU6YMGK2` |
| `por_verifier` | `CASW45LEE4ZX5PZ2BDFS3FSLAWUSDTISB35WOQ7IIUTSJPE4V3W7WRUH` |
| `eligibility_verifier` | `CCWJBZ55J2K243ZLK4PAYC5XXE5HYF7DJE5SGYXI4X7CHCYWNF5UDML4` |
| `rwa_gate` | `CC6G23ZWQTLK72B5BNF6OUBYXX3XQQZ2YTUYJFOVLGEWRO5W57YEN2HY` |

View on [Stellar Expert](https://stellar.expert/explorer/testnet/contract/CBCZQMNXATGWCKTZPEXYFA7MO4R7EULQP4LRHWBCORPGONMMWU6YMGK2).

---

## Repository layout

```
aegis/
├── circuits/                 # Circom ZK circuits + circomlib include shims
├── contracts/                # Four Soroban contracts (Rust) + native tests
│   ├── groth16_bn254_verifier/  # Self-contained BN254 Groth16 verifier
│   ├── por_verifier/            # Proof-of-Reserves attestation
│   ├── eligibility_verifier/    # Selective-disclosure gate
│   └── rwa_gate/                # Composing gate
├── prover/                   # snarkjs prover, credential issuer, Soroban formatter, tests
├── scripts/                  # build-circuits / deploy / e2e-demo / invoke-onchain / export-vk / encode-invoke-args / register-vk
├── frontend/                 # single-file demo UI
├── docs/                     # SETUP.md, ARCHITECTURE.md, UPGRADE.md, GROTH16_VERIFIER.md
└── .github/workflows/ci.yml  # prover tests + contract build/test
```

## License

MIT — see [`LICENSE`](LICENSE).
