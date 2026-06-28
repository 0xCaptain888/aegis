#![cfg(test)]
//! Tests for PorVerifier. We deploy a MOCK groth16 verifier (returns a
//! configurable result) so we can test the contract's binding logic — commitment
//! match, supply match, bps match — independently of the real pairing check.
//! The real pairing check is exercised by the end-to-end script against testnet.

use super::*;
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, vec, Address, BytesN, Env, Vec,
};

// ---- Mock verifier contract ----
#[contract]
pub struct MockVerifier;

#[contractimpl]
impl MockVerifier {
    // IMPORTANT: this signature must match exactly what `groth16::verify_via_contract`
    // sends on the wire — five positional args: (vk_id, proof_a, proof_b, proof_c,
    // public_inputs) — NOT a single Groth16Proof struct. An earlier version of this
    // mock took `(_vk_id, _proof: Groth16Proof, _signals)` (3 args); a Soroban
    // cross-contract call with a different arg count traps, so the tests would fail
    // at runtime even though they compiled. Keep this aligned with the real
    // groth16_bn254_verifier::verify signature.
    pub fn verify(
        env: Env,
        _vk_id: u32,
        _proof_a: BytesN<64>,
        _proof_b: BytesN<128>,
        _proof_c: BytesN<64>,
        _public_inputs: PublicSignals,
    ) -> bool {
        // configurable via instance storage; defaults to true
        env.storage()
            .instance()
            .get(&symbol_short!("ok"))
            .unwrap_or(true)
    }
    pub fn set_ok(env: Env, ok: bool) {
        env.storage().instance().set(&symbol_short!("ok"), &ok);
    }
}
use soroban_sdk::symbol_short;

fn be32_from_u128(env: &Env, v: u128) -> BytesN<32> {
    let mut arr = [0u8; 32];
    let mut x = v;
    for i in (16..32).rev() {
        arr[i] = (x & 0xff) as u8;
        x >>= 8;
    }
    BytesN::from_array(env, &arr)
}

fn dummy_proof(env: &Env) -> Groth16Proof {
    Groth16Proof {
        a: BytesN::from_array(env, &[1u8; 64]),
        b: BytesN::from_array(env, &[2u8; 128]),
        c: BytesN::from_array(env, &[3u8; 64]),
    }
}

fn setup() -> (Env, PorVerifierClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = Address::generate(&env);

    let verifier_id = env.register(MockVerifier, ());
    let contract_id = env.register(PorVerifier, ());
    let client = PorVerifierClient::new(&env, &contract_id);

    client.init(&admin, &verifier_id);

    (env, client, issuer, token, verifier_id)
}

#[test]
fn attests_when_everything_matches() {
    let (env, client, issuer, token, _v) = setup();

    let commitment = BytesN::from_array(&env, &[7u8; 32]);
    let bps = 10000u32;
    let supply = 9_000_000i128;

    client.set_policy(&token, &issuer, &commitment, &bps, &0u32);

    let signals: Vec<BytesN<32>> = vec![
        &env,
        be32_from_u128(&env, supply as u128), // [0] totalSupply
        commitment.clone(),                   // [1] reservesCommitment
        be32_from_u128(&env, bps as u128),    // [2] minCollateralBps
    ];

    let att = client.attest(&token, &supply, &dummy_proof(&env), &signals);
    assert_eq!(att.total_supply, supply);
    assert_eq!(att.min_collateral_bps, bps);

    let last = client.last_attestation(&token).unwrap();
    assert_eq!(last.total_supply, supply);
}

#[test]
fn rejects_commitment_mismatch() {
    let (env, client, issuer, token, _v) = setup();
    let commitment = BytesN::from_array(&env, &[7u8; 32]);
    client.set_policy(&token, &issuer, &commitment, &10000u32, &0u32);

    let wrong = BytesN::from_array(&env, &[9u8; 32]);
    let signals: Vec<BytesN<32>> = vec![
        &env,
        be32_from_u128(&env, 9_000_000u128),
        wrong, // mismatched commitment
        be32_from_u128(&env, 10000u128),
    ];
    let res = client.try_attest(&token, &9_000_000i128, &dummy_proof(&env), &signals);
    assert!(res.is_err());
}

#[test]
fn rejects_supply_mismatch() {
    let (env, client, issuer, token, _v) = setup();
    let commitment = BytesN::from_array(&env, &[7u8; 32]);
    client.set_policy(&token, &issuer, &commitment, &10000u32, &0u32);

    let signals: Vec<BytesN<32>> = vec![
        &env,
        be32_from_u128(&env, 9_000_000u128), // signal supply
        commitment.clone(),
        be32_from_u128(&env, 10000u128),
    ];
    // claimed supply differs from signal
    let res = client.try_attest(&token, &8_000_000i128, &dummy_proof(&env), &signals);
    assert!(res.is_err());
}

#[test]
fn rejects_when_proof_invalid() {
    let (env, client, issuer, token, verifier_id) = setup();
    let commitment = BytesN::from_array(&env, &[7u8; 32]);
    client.set_policy(&token, &issuer, &commitment, &10000u32, &0u32);

    // make the mock verifier reject
    let mv = MockVerifierClient::new(&env, &verifier_id);
    mv.set_ok(&false);

    let signals: Vec<BytesN<32>> = vec![
        &env,
        be32_from_u128(&env, 9_000_000u128),
        commitment.clone(),
        be32_from_u128(&env, 10000u128),
    ];
    let res = client.try_attest(&token, &9_000_000i128, &dummy_proof(&env), &signals);
    assert!(res.is_err());
}
