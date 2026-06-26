//! groth16.rs — BN254 Groth16 verification helpers for Soroban.
//!
//! This module is the SINGLE place where on-chain proof verification happens.
//! Stellar Protocol 25 (X-Ray) added native BN254 host functions (pairing,
//! G1/G2 ops) and Protocol 26 (Yardstick) added MSM + field ops, which is what
//! makes Groth16 verification cheap enough to run inside a contract.
//!
//! There are two ways the verification call can be wired, depending on the SDK /
//! verifier you target. We expose ONE function, `verify_groth16`, and document
//! both wirings so you can pick the one your toolchain supports:
//!
//!   (A) Cross-contract call into the community `groth16_verifier` contract
//!       (stellar/soroban-examples) deployed separately. Recommended: it tracks
//!       the host-function ABI for you. You pass its address at init time.
//!
//!   (B) Direct host-function pairing check, if your soroban-sdk version exposes
//!       the BN254 crypto API directly (e.g. env.crypto().bn254_*). The exact
//!       method names are SDK-version-specific; see docs/UPGRADE.md.
//!
//! The default build uses path (A) so the contracts compile against a stable
//! soroban-sdk surface without depending on host method names that move between
//! SDK minor versions. Flip to (B) by enabling the `host_pairing` feature and
//! filling in the marked block.

use soroban_sdk::{contracttype, vec, Address, Bytes, BytesN, Env, IntoVal, Val, Vec};

/// A Groth16 proof in the byte layout produced by `@aegis/prover`
/// (soroban-format.js): a, c are G1 (64 bytes), b is G2 (128 bytes).
#[contracttype]
#[derive(Clone)]
pub struct Groth16Proof {
    pub a: BytesN<64>,
    pub b: BytesN<128>,
    pub c: BytesN<64>,
}

/// Public signals as 32-byte big-endian field elements, in circuit order.
pub type PublicSignals = Vec<BytesN<32>>;

/// Verifies a Groth16 proof by delegating to a deployed `groth16_verifier`
/// contract (path A). `verifier` is the address of that contract; `vk_id` lets
/// the verifier select the right verification key (one per circuit).
///
/// Returns true iff the proof verifies against the registered VK and signals.
#[allow(dead_code)]
pub fn verify_via_contract(
    env: &Env,
    verifier: &Address,
    vk_id: u32,
    proof: &Groth16Proof,
    signals: &Vec<BytesN<32>>,
) -> bool {
    // Cross-contract invocation. The community verifier exposes a `verify`
    // function with this shape. If your verifier's signature differs, this is
    // the one line to adapt (see docs/UPGRADE.md "Wiring the verifier").
    let args: Vec<Val> = vec![
        env,
        vk_id.into_val(env),
        proof.clone().into_val(env),
        signals.clone().into_val(env),
    ];
    env.invoke_contract::<bool>(verifier, &soroban_sdk::Symbol::new(env, "verify"), args)
}

/// Convenience: pack the raw proof bytes (as emitted by the prover) into a
/// Groth16Proof. `blob` must be exactly 256 bytes (64 + 128 + 64).
#[allow(dead_code)]
pub fn proof_from_bytes(env: &Env, blob: &Bytes) -> Groth16Proof {
    assert!(blob.len() == 256, "proof blob must be 256 bytes");
    let mut a = [0u8; 64];
    let mut b = [0u8; 128];
    let mut c = [0u8; 64];
    let slice = blob.clone();
    for i in 0..64u32 {
        a[i as usize] = slice.get(i).unwrap();
    }
    for i in 0..128u32 {
        b[i as usize] = slice.get(64 + i).unwrap();
    }
    for i in 0..64u32 {
        c[i as usize] = slice.get(192 + i).unwrap();
    }
    Groth16Proof {
        a: BytesN::from_array(env, &a),
        b: BytesN::from_array(env, &b),
        c: BytesN::from_array(env, &c),
    }
}
