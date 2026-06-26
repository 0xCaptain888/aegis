#![no_std]
//! Aegis — Self-Contained BN254 Groth16 Verifier (Soroban Contract)
//!
//! This is the FOURTH contract in the Aegis stack. Unlike the three
//! application contracts (por_verifier, eligibility_verifier, rwa_gate), this
//! one has no business logic: it is a pure on-chain cryptographic library that
//! verifies BN254 Groth16 proofs.
//!
//! **Why a dedicated contract?**
//! Stellar Protocol 25 (X-Ray, CAP-0074) added native BN254 host functions
//! (`bn254_g1_add`, `bn254_g1_mul`, `bn254_multi_pairing_check`) that make
//! Groth16 verification cheap inside a Soroban contract. Rather than relying on
//! a community contract deployed by a third party, Aegis ships its own verifier
//! in the same repository so the build is fully self-contained — no external
//! address, no external trust assumption.
//!
//! **Deployment order:**
//!   1. Deploy this contract → `GROTH16_ID`
//!   2. Register each circuit's VK with `register_vk(vk_id, alpha, beta, gamma, delta, ic)`
//!   3. Deploy `por_verifier` / `eligibility_verifier` / `rwa_gate`, passing `GROTH16_ID`
//!
//! **Interface:**
//!   - `register_vk(vk_id, alpha, beta, gamma, delta, ic)` — admin-only, stores a VK
//!   - `verify(vk_id, proof_a, proof_b, proof_c, public_inputs)` → bool
//!
//! **Groth16 pairing check (BN254):**
//! A proof (A, B, C) is valid iff:
//!   e(A, B) = e(α, β) · e(Σ, γ) · e(C, δ)
//! where Σ = Σᵢ (sᵢ · ICᵢ), sᵢ are the public inputs (plus 1 prepended),
//! and IC is the input commitment key.
//!
//! Equivalently (via multi-pairing check):
//!   e(-A, B) · e(α, β) · e(Σ, γ) · e(C, δ) = 1
//!
//! This contract encodes that as:
//!   multi_pairing_check([-A, α, Σ, C], [B, β, γ, δ]) == true
//!
//! **Note on soroban-sdk version compatibility:**
//!
//! soroban-sdk 22.x ships with BN254 support accessible through
//! `env.crypto_hazmat()` under the `hazmat-crypto` feature. If you build with
//! soroban-sdk ≥ 25.x, enable the `host-pairing` feature in Cargo.toml for a
//! faster path that avoids the Wasm-side helper math below.
//!
//! The verify() implementation here uses the `bn254_multi_pairing_check` host
//! function via the env crypto API. Byte format: all points are uncompressed
//! big-endian: G1 = 64 bytes (32 x + 32 y), G2 = 128 bytes (32 x1 32 x0 32 y1 32 y0).
//! This matches what `prover/src/soroban-format.js` emits.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short,
    Address, BytesN, Env, Vec,
};

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    VkNotFound = 4,
    InvalidInputCount = 5,
    ProofInvalid = 6,
}

// ---------------------------------------------------------------------------
// Storage types
// ---------------------------------------------------------------------------

/// A Groth16 verification key stored on-chain.
/// All points in big-endian uncompressed byte format (G1=64B, G2=128B).
#[contracttype]
#[derive(Clone)]
pub struct VerificationKey {
    /// α ∈ G1 (64 bytes)
    pub alpha: BytesN<64>,
    /// β ∈ G2 (128 bytes)
    pub beta: BytesN<128>,
    /// γ ∈ G2 (128 bytes)
    pub gamma: BytesN<128>,
    /// δ ∈ G2 (128 bytes)
    pub delta: BytesN<128>,
    /// IC (input commitment key), length = n_public + 1; each ∈ G1 (64 bytes)
    pub ic: Vec<BytesN<64>>,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Vk(u32), // vk_id -> VerificationKey
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Groth16Bn254Verifier;

#[contractimpl]
impl Groth16Bn254Verifier {
    // -----------------------------------------------------------------------
    // Admin / init
    // -----------------------------------------------------------------------

    /// One-time initialization. Must be called immediately after deployment.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register a Groth16 verification key for one circuit.
    /// `vk_id` is an arbitrary u32 chosen by the deployer (0 = PoR, 1 = Eligibility).
    pub fn register_vk(
        env: Env,
        vk_id: u32,
        alpha: BytesN<64>,
        beta: BytesN<128>,
        gamma: BytesN<128>,
        delta: BytesN<128>,
        ic: Vec<BytesN<64>>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let vk = VerificationKey { alpha, beta, gamma, delta, ic };
        env.storage().persistent().set(&DataKey::Vk(vk_id), &vk);
        env.events().publish(
            (symbol_short!("vk_set"), vk_id),
            (),
        );
        Ok(())
    }

    /// Read back a registered VK (for inspection / debugging).
    pub fn vk(env: Env, vk_id: u32) -> Option<VerificationKey> {
        env.storage().persistent().get(&DataKey::Vk(vk_id))
    }

    // -----------------------------------------------------------------------
    // Core verification
    // -----------------------------------------------------------------------

    /// Verify a Groth16 BN254 proof.
    ///
    /// Arguments:
    ///   - `vk_id`         : which VK to use (registered via `register_vk`)
    ///   - `proof_a`       : A ∈ G1  (64 bytes, big-endian uncompressed)
    ///   - `proof_b`       : B ∈ G2  (128 bytes)
    ///   - `proof_c`       : C ∈ G1  (64 bytes)
    ///   - `public_inputs` : Vec of n_public 32-byte big-endian field elements
    ///
    /// Returns true iff the proof is valid, false otherwise. Never panics on a
    /// bad proof (only on missing VK or wrong input count).
    pub fn verify(
        env: Env,
        vk_id: u32,
        proof_a: BytesN<64>,
        proof_b: BytesN<128>,
        proof_c: BytesN<64>,
        public_inputs: Vec<BytesN<32>>,
    ) -> Result<bool, Error> {
        let vk: VerificationKey = env
            .storage()
            .persistent()
            .get(&DataKey::Vk(vk_id))
            .ok_or(Error::VkNotFound)?;

        // IC has length n_public + 1; the extra element is for the constant 1.
        let n_public = (vk.ic.len() - 1) as usize;
        if public_inputs.len() as usize != n_public {
            return Err(Error::InvalidInputCount);
        }

        // --- Compute Σ = IC[0] + Σᵢ (sᵢ · IC[i+1]) ---
        // Using the BN254 host functions: g1_add and g1_mul.
        // Each operation uses the env crypto hazmat API.
        let sigma = compute_sigma(&env, &vk.ic, &public_inputs);

        // --- Build the 4-pairing multi-check ---
        // We compute: e(-A, B) · e(α, β) · e(Σ, γ) · e(C, δ) == 1
        // Negate A by flipping its Y coordinate (on BN254: y_neg = p - y).
        let neg_a = g1_negate(&env, &proof_a);

        // Pack into Vec<BytesN<64>> (G1) and Vec<BytesN<128>> (G2).
        let g1_points: Vec<BytesN<64>> = {
            let mut v = Vec::new(&env);
            v.push_back(neg_a);           // -A
            v.push_back(vk.alpha.clone()); // α
            v.push_back(sigma);            // Σ
            v.push_back(proof_c);          // C
            v
        };
        let g2_points: Vec<BytesN<128>> = {
            let mut v = Vec::new(&env);
            v.push_back(proof_b);          // B
            v.push_back(vk.beta.clone());  // β
            v.push_back(vk.gamma.clone()); // γ
            v.push_back(vk.delta.clone()); // δ
            v
        };

        // Call the BN254 multi-pairing check host function.
        // Available in soroban-sdk ≥ 22.x via env.crypto_hazmat()
        // (feature: hazmat-crypto, or unconditionally in 25.x+).
        let ok = bn254_multi_pairing_check(&env, &g1_points, &g2_points);
        Ok(ok)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Compute Σ = IC[0] + s[0]·IC[1] + s[1]·IC[2] + … + s[n-1]·IC[n]
/// using BN254 G1 scalar-mul and point-addition host functions.
fn compute_sigma(
    env: &Env,
    ic: &Vec<BytesN<64>>,
    scalars: &Vec<BytesN<32>>,
) -> BytesN<64> {
    // Start with IC[0] (the "1" element).
    let mut acc: BytesN<64> = ic.get(0).unwrap();

    for i in 0..scalars.len() {
        let ic_i: BytesN<64> = ic.get(i + 1).unwrap();
        let s: BytesN<32>    = scalars.get(i).unwrap();
        // scalar_mul: s·IC[i+1]
        let term = bn254_g1_mul(env, &ic_i, &s);
        // add: acc = acc + term
        acc = bn254_g1_add(env, &acc, &term);
    }
    acc
}

/// Negate a G1 point by flipping Y: (x, y) → (x, p−y).
/// BN254 field prime p = 21888242871839275222246405745257275088696311157297823662689037894645226208583.
/// Points are big-endian 32-byte x ‖ 32-byte y.
fn g1_negate(env: &Env, pt: &BytesN<64>) -> BytesN<64> {
    // p as 32 big-endian bytes.
    const P: [u8; 32] = [
        0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29,
        0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
        0x97, 0x81, 0x6a, 0x91, 0x68, 0x71, 0xca, 0x8d,
        0x3c, 0x20, 0x8c, 0x16, 0xd8, 0x7c, 0xfd, 0x47,
    ];

    // Extract x (bytes 0..32) and y (bytes 32..64).
    let mut x = [0u8; 32];
    let mut y = [0u8; 32];
    for i in 0u32..32 {
        x[i as usize] = pt.get(i).unwrap();
        y[i as usize] = pt.get(32 + i).unwrap();
    }

    // Compute p − y using big-endian subtraction.
    let neg_y = be32_sub(&P, &y);

    let mut out = [0u8; 64];
    out[0..32].copy_from_slice(&x);
    out[32..64].copy_from_slice(&neg_y);
    BytesN::from_array(env, &out)
}

/// Big-endian 32-byte subtraction: a − b (assumes a ≥ b, result < 2^256).
fn be32_sub(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let mut borrow: u16 = 0;
    for i in (0..32).rev() {
        let diff = (a[i] as u16).wrapping_sub(b[i] as u16).wrapping_sub(borrow);
        result[i] = diff as u8;
        borrow = if diff > 0xff { 1 } else { 0 };
    }
    result
}

// ---------------------------------------------------------------------------
// BN254 operations using soroban-sdk 25.x API.
//
// The SDK provides `env.crypto().bn254()` which returns a `Bn254` struct with methods:
// - g1_add(p0: Bn254G1Affine, p1: Bn254G1Affine) -> Bn254G1Affine
// - g1_mul(p0: Bn254G1Affine, scalar: Fr) -> Bn254G1Affine
// - pairing_check(vp1: Vec<Bn254G1Affine>, vp2: Vec<Bn254G2Affine>) -> bool
//
// We wrap these to convert between raw bytes and the typed structs.
// ---------------------------------------------------------------------------

use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine, Fr};

fn bn254_g1_add(env: &Env, p1: &BytesN<64>, p2: &BytesN<64>) -> BytesN<64> {
    let bn254 = env.crypto().bn254();
    let point1 = Bn254G1Affine::from_bytes(p1.clone());
    let point2 = Bn254G1Affine::from_bytes(p2.clone());
    let result = bn254.g1_add(&point1, &point2);
    result.to_bytes()
}

fn bn254_g1_mul(env: &Env, pt: &BytesN<64>, scalar: &BytesN<32>) -> BytesN<64> {
    let bn254 = env.crypto().bn254();
    let point = Bn254G1Affine::from_bytes(pt.clone());
    // Convert BytesN<32> to Fr
    let fr_scalar = Fr::from_bytes(scalar.clone());
    let result = bn254.g1_mul(&point, &fr_scalar);
    result.to_bytes()
}

fn bn254_multi_pairing_check(
    env: &Env,
    g1: &Vec<BytesN<64>>,
    g2: &Vec<BytesN<128>>,
) -> bool {
    let bn254 = env.crypto().bn254();
    
    // Convert Vec<BytesN<64>> to Vec<Bn254G1Affine>
    let mut g1_points: Vec<Bn254G1Affine> = Vec::new(env);
    for i in 0..g1.len() {
        let point_bytes = g1.get(i).unwrap();
        g1_points.push_back(Bn254G1Affine::from_bytes(point_bytes));
    }
    
    // Convert Vec<BytesN<128>> to Vec<Bn254G2Affine>
    let mut g2_points: Vec<Bn254G2Affine> = Vec::new(env);
    for i in 0..g2.len() {
        let point_bytes = g2.get(i).unwrap();
        g2_points.push_back(Bn254G2Affine::from_bytes(point_bytes));
    }
    
    bn254.pairing_check(g1_points, g2_points)
}

// ---------------------------------------------------------------------------
// Tests (use soroban-sdk testutils mock host)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
