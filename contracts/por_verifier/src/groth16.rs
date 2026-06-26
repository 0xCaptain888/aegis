//! groth16.rs — BN254 Groth16 verification bridge for Soroban.
//!
//! This module is the single place where the application contracts call the
//! on-chain BN254 Groth16 verifier. Aegis ships its own verifier contract
//! (`groth16_bn254_verifier`) in the same repository, so no external address
//! or external trust assumption is needed — the entire stack deploys from one
//! repo with `scripts/deploy.sh`.
//!
//! Verification flow:
//!   Application contract (e.g. por_verifier)
//!     → verify_via_contract()          [this file]
//!       → cross-contract call to groth16_bn254_verifier.verify()
//!         → bn254_g1_mul / bn254_g1_add / bn254_multi_pairing_check host fns
//!           (Protocol 25 CAP-0074, available in soroban-sdk ≥ 22.x)
//!
//! To swap the verifier (e.g. to a community contract or an upgraded version),
//! change only `verify_via_contract` — all application logic above stays the same.

use soroban_sdk::{vec, Address, BytesN, Env, IntoVal, Val, Vec};

/// A Groth16 proof in the byte layout produced by `prover/src/soroban-format.js`:
///   a: G1 (64 bytes)   = BE(x) ‖ BE(y)
///   b: G2 (128 bytes)  = BE(x1) ‖ BE(x0) ‖ BE(y1) ‖ BE(y0)  [G2_FP2_ORDER knob]
///   c: G1 (64 bytes)
#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Groth16Proof {
    pub a: BytesN<64>,
    pub b: BytesN<128>,
    pub c: BytesN<64>,
}

/// Public signals as 32-byte big-endian field elements, in circuit order.
pub type PublicSignals = Vec<BytesN<32>>;

/// Verify a Groth16 BN254 proof by cross-calling the Aegis `groth16_bn254_verifier`
/// contract. `verifier` is the contract's address (stored in this contract at init
/// time and passed in here). `vk_id` selects the right VK (0=PoR, 1=Eligibility).
///
/// Returns true iff the proof is valid; false on a bad proof. Propagates panics
/// from the host (e.g. point not on curve) as contract errors.
pub fn verify_via_contract(
    env: &Env,
    verifier: &Address,
    vk_id: u32,
    proof: &Groth16Proof,
    signals: &PublicSignals,
) -> bool {
    // The groth16_bn254_verifier.verify() signature:
    //   fn verify(vk_id: u32, proof_a: BytesN<64>, proof_b: BytesN<128>,
    //             proof_c: BytesN<64>, public_inputs: Vec<BytesN<32>>) -> Result<bool, Error>
    let args: Vec<Val> = vec![
        env,
        vk_id.into_val(env),
        proof.a.clone().into_val(env),
        proof.b.clone().into_val(env),
        proof.c.clone().into_val(env),
        signals.clone().into_val(env),
    ];
    env.invoke_contract::<bool>(
        verifier,
        &soroban_sdk::Symbol::new(env, "verify"),
        args,
    )
}

/// Pack raw proof bytes (256 bytes: 64+128+64) into a Groth16Proof struct.
/// Convenience for off-chain tooling that serializes proofs as a single blob.
#[allow(dead_code)]
pub fn proof_from_bytes(env: &Env, blob: &soroban_sdk::Bytes) -> Groth16Proof {
    assert!(blob.len() == 256, "proof blob must be 256 bytes (64+128+64)");
    let mut a = [0u8; 64];
    let mut b = [0u8; 128];
    let mut c = [0u8; 64];
    for i in 0..64u32  { a[i as usize] = blob.get(i).unwrap(); }
    for i in 0..128u32 { b[i as usize] = blob.get(64 + i).unwrap(); }
    for i in 0..64u32  { c[i as usize] = blob.get(192 + i).unwrap(); }
    Groth16Proof {
        a: BytesN::from_array(env, &a),
        b: BytesN::from_array(env, &b),
        c: BytesN::from_array(env, &c),
    }
}
