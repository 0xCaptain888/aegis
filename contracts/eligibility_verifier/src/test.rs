#![cfg(test)]
use super::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::{Address as _, Ledger}, vec, Address, BytesN,
    Env, Vec,
};

#[contract]
pub struct MockVerifier;

#[contractimpl]
impl MockVerifier {
    pub fn verify(env: Env, _vk_id: u32, _p: Groth16Proof, _s: PublicSignals) -> bool {
        env.storage().instance().get(&symbol_short!("ok")).unwrap_or(true)
    }
    pub fn set_ok(env: Env, ok: bool) {
        env.storage().instance().set(&symbol_short!("ok"), &ok);
    }
}

fn b32(env: &Env, fill: u8) -> BytesN<32> {
    BytesN::from_array(env, &[fill; 32])
}
fn be32_u64(env: &Env, v: u64) -> BytesN<32> {
    let mut a = [0u8; 32];
    let mut x = v;
    for i in (24..32).rev() {
        a[i] = (x & 0xff) as u8;
        x >>= 8;
    }
    BytesN::from_array(env, &a)
}
fn be32_u32(env: &Env, v: u32) -> BytesN<32> {
    be32_u64(env, v as u64)
}
fn dummy_proof(env: &Env) -> Groth16Proof {
    Groth16Proof {
        a: BytesN::from_array(env, &[1u8; 64]),
        b: BytesN::from_array(env, &[2u8; 128]),
        c: BytesN::from_array(env, &[3u8; 64]),
    }
}

struct Ctx {
    env: Env,
    client: EligibilityVerifierClient<'static>,
    verifier_id: Address,
    admin: Address,
    gate_id: BytesN<32>,
    pkx: BytesN<32>,
    pky: BytesN<32>,
    root: BytesN<32>,
    action: BytesN<32>,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let verifier_id = env.register(MockVerifier, ());
    let cid = env.register(EligibilityVerifier, ());
    let client = EligibilityVerifierClient::new(&env, &cid);
    client.init(&admin, &verifier_id);

    let pkx = b32(&env, 0xAA);
    let pky = b32(&env, 0xBB);
    let root = b32(&env, 0xCC);
    let action = b32(&env, 0xDD);
    let gate_id = b32(&env, 0x01);

    let policy = GatePolicy {
        issuer_pubkey_x: pkx.clone(),
        issuer_pubkey_y: pky.clone(),
        required_kyc_level: 2,
        require_accredited: true,
        allowed_jurisdiction_root: root.clone(),
        action_id: action.clone(),
        vk_id: 0,
        max_skew_secs: 3600,
    };
    client.set_gate(&admin, &gate_id, &policy);

    Ctx { env, client, verifier_id, admin, gate_id, pkx, pky, root, action }
}

fn good_signals(c: &Ctx, nullifier: BytesN<32>) -> Vec<BytesN<32>> {
    vec![
        &c.env,
        c.pkx.clone(),
        c.pky.clone(),
        be32_u32(&c.env, 2),       // requiredKyc
        be32_u32(&c.env, 1),       // requireAccredited = true
        c.root.clone(),
        be32_u64(&c.env, 1_700_000_000), // timestamp == ledger
        c.action.clone(),
        nullifier,
    ]
}

#[test]
fn verifies_and_consumes_nullifier() {
    let c = setup();
    let n = b32(&c.env, 0x42);
    let sigs = good_signals(&c, n.clone());
    let got = c.client.verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &sigs);
    assert_eq!(got, n);
    assert!(c.client.is_nullifier_used(&c.gate_id, &n));
}

#[test]
fn rejects_double_use() {
    let c = setup();
    let n = b32(&c.env, 0x42);
    let sigs = good_signals(&c, n.clone());
    c.client.verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &sigs);
    // second use of same nullifier must fail
    let res = c.client.try_verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &good_signals(&c, n));
    assert!(res.is_err());
}

#[test]
fn rejects_policy_mismatch_kyc() {
    let c = setup();
    let n = b32(&c.env, 0x43);
    let mut sigs = good_signals(&c, n);
    sigs.set(2, be32_u32(&c.env, 1)); // required kyc 1 != policy 2
    let res = c.client.try_verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &sigs);
    assert!(res.is_err());
}

#[test]
fn rejects_stale_timestamp() {
    let c = setup();
    let n = b32(&c.env, 0x44);
    let mut sigs = good_signals(&c, n);
    sigs.set(5, be32_u64(&c.env, 1_600_000_000)); // way outside skew
    let res = c.client.try_verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &sigs);
    assert!(res.is_err());
}

#[test]
fn rejects_invalid_proof() {
    let c = setup();
    let mv = MockVerifierClient::new(&c.env, &c.verifier_id);
    mv.set_ok(&false);
    let n = b32(&c.env, 0x45);
    let sigs = good_signals(&c, n);
    let res = c.client.try_verify_eligibility(&c.gate_id, &dummy_proof(&c.env), &sigs);
    assert!(res.is_err());
}
