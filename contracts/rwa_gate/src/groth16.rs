//! groth16.rs — BN254 Groth16 verification bridge for Soroban.
//! (rwa_gate re-export — this contract does NOT call verify directly; it
//! delegates to eligibility_verifier via cross-contract call, which in turn
//! calls groth16_bn254_verifier. The shared types are kept here for
//! consistency and for the Groth16Proof contracttype encoding.)

use soroban_sdk::{vec, Address, BytesN, Env, IntoVal, Val, Vec};

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Groth16Proof {
    pub a: BytesN<64>,
    pub b: BytesN<128>,
    pub c: BytesN<64>,
}

pub type PublicSignals = Vec<BytesN<32>>;

#[allow(dead_code)]
pub fn verify_via_contract(
    env: &Env,
    verifier: &Address,
    vk_id: u32,
    proof: &Groth16Proof,
    signals: &PublicSignals,
) -> bool {
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

#[allow(dead_code)]
pub fn proof_from_bytes(env: &Env, blob: &soroban_sdk::Bytes) -> Groth16Proof {
    assert!(blob.len() == 256, "proof blob must be 256 bytes");
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
