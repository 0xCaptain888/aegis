#![cfg(test)]
//! Tests for the groth16_bn254_verifier contract.
//!
//! NOTE: The soroban-sdk testutils mock host does NOT implement the BN254
//! crypto host functions (they are only available in an actual Stellar node /
//! the full host for SDK 22.x). Tests here therefore focus on:
//!   1. Admin / init flow (pure storage, no crypto).
//!   2. VK registration and retrieval.
//!   3. Input-count mismatch rejection (caught before pairing).
//!   4. VkNotFound rejection.
//!
//! The actual pairing check is validated end-to-end by `scripts/e2e-demo.sh` +
//! `scripts/invoke-onchain.sh` against the deployed testnet contract.

use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env};

use crate::{Error, Groth16Bn254Verifier, VerificationKey};

fn g1_zero(env: &Env) -> BytesN<64> { BytesN::from_array(env, &[0u8; 64]) }
fn g2_zero(env: &Env) -> BytesN<128> { BytesN::from_array(env, &[0u8; 128]) }
fn fr_one(env: &Env) -> BytesN<32> {
    let mut b = [0u8; 32]; b[31] = 1;
    BytesN::from_array(env, &b)
}

fn setup() -> (Env, Address, crate::Groth16Bn254VerifierClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let cid = env.register(Groth16Bn254Verifier, ());
    let client = crate::Groth16Bn254VerifierClient::new(&env, &cid);
    client.init(&admin);
    (env, admin, client)
}

#[test]
fn init_and_double_init() {
    let (_, _, client) = setup();
    // second init must fail
    let admin2 = Address::generate(&client.env);
    let res = client.try_init(&admin2);
    assert!(matches!(
        res,
        Err(Ok(crate::Error::AlreadyInitialized))
    ));
}

#[test]
fn register_and_read_vk() {
    let (env, _, client) = setup();
    let vk_id: u32 = 0;
    let ic = vec![&env, g1_zero(&env), g1_zero(&env)]; // 1 public input
    client.register_vk(
        &vk_id,
        &g1_zero(&env),
        &g2_zero(&env),
        &g2_zero(&env),
        &g2_zero(&env),
        &ic,
    );
    let fetched: Option<VerificationKey> = client.vk(&vk_id);
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().ic.len(), 2);
}

#[test]
fn verify_vk_not_found() {
    let (env, _, client) = setup();
    let proof_a = g1_zero(&env);
    let proof_b = g2_zero(&env);
    let proof_c = g1_zero(&env);
    let inputs = vec![&env, fr_one(&env)];
    let res = client.try_verify(&99u32, &proof_a, &proof_b, &proof_c, &inputs);
    assert!(matches!(res, Err(Ok(Error::VkNotFound))));
}

#[test]
fn verify_wrong_input_count() {
    let (env, _, client) = setup();
    let vk_id: u32 = 7;
    let ic = vec![&env, g1_zero(&env), g1_zero(&env)]; // 1 public input
    client.register_vk(&vk_id, &g1_zero(&env), &g2_zero(&env), &g2_zero(&env), &g2_zero(&env), &ic);

    // Pass 2 inputs instead of 1 → InvalidInputCount
    let inputs = vec![&env, fr_one(&env), fr_one(&env)];
    let res = client.try_verify(&vk_id, &g1_zero(&env), &g2_zero(&env), &g1_zero(&env), &inputs);
    assert!(matches!(res, Err(Ok(Error::InvalidInputCount))));
}

#[test]
fn register_before_init_fails() {
    // A fresh contract that was never init()'d has no Admin stored, so
    // register_vk must reject with NotInitialized (the admin lookup fails
    // before any auth check). This replaces an earlier version of this test
    // that incorrectly tried to construct the `#[contract]` unit struct with
    // fields — that does not compile, because the client type is
    // `Groth16Bn254VerifierClient`, not the contract struct itself.
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Groth16Bn254Verifier, ());
    let client = crate::Groth16Bn254VerifierClient::new(&env, &cid);

    let ic = vec![&env, g1_zero(&env), g1_zero(&env)];
    let res = client.try_register_vk(
        &0u32,
        &g1_zero(&env),
        &g2_zero(&env),
        &g2_zero(&env),
        &g2_zero(&env),
        &ic,
    );
    assert!(matches!(res, Err(Ok(Error::NotInitialized))));
}
