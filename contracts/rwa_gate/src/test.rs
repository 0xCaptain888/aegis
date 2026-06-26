#![cfg(test)]
use super::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::{Address as _, Ledger}, vec, Address, BytesN,
    Env, Vec,
};

// ---- Mock PoR verifier exposing last_attestation ----
#[contract]
pub struct MockPor;

#[contractimpl]
impl MockPor {
    pub fn set_att(env: Env, att: Attestation) {
        env.storage().instance().set(&symbol_short!("att"), &att);
    }
    pub fn last_attestation(env: Env, _token: Address) -> Option<Attestation> {
        env.storage().instance().get(&symbol_short!("att"))
    }
}

// ---- Mock eligibility verifier returning a nullifier or panicking ----
#[contract]
pub struct MockElig;

#[contractimpl]
impl MockElig {
    pub fn set_ok(env: Env, ok: bool) {
        env.storage().instance().set(&symbol_short!("ok"), &ok);
    }
    pub fn verify_eligibility(
        env: Env,
        _gate_id: BytesN<32>,
        _p: Groth16Proof,
        _s: PublicSignals,
    ) -> BytesN<32> {
        let ok: bool = env.storage().instance().get(&symbol_short!("ok")).unwrap_or(true);
        if !ok {
            panic!("eligibility failed");
        }
        BytesN::from_array(&env, &[0x99u8; 32])
    }
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
    client: RwaGateClient<'static>,
    admin: Address,
    token: Address,
    por_id: Address,
    elig_id: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let por_id = env.register(MockPor, ());
    let elig_id = env.register(MockElig, ());

    let cid = env.register(RwaGate, ());
    let client = RwaGateClient::new(&env, &cid);
    client.init(&admin);

    let cfg = GateConfig {
        token: token.clone(),
        por_verifier: por_id.clone(),
        eligibility_verifier: elig_id.clone(),
        eligibility_gate_id: BytesN::from_array(&env, &[0x01; 32]),
        max_reserve_age_secs: 86_400, // 1 day
    };
    client.configure(&admin, &cfg);

    Ctx { env, client, admin, token, por_id, elig_id }
}

fn put_attestation(c: &Ctx, verified_at: u64) {
    let por = MockPorClient::new(&c.env, &c.por_id);
    por.set_att(&Attestation {
        token: c.token.clone(),
        total_supply: 9_000_000,
        reserves_commitment: BytesN::from_array(&c.env, &[7u8; 32]),
        min_collateral_bps: 10000,
        verified_at,
    });
}

#[test]
fn authorizes_when_reserves_fresh_and_eligible() {
    let c = setup();
    put_attestation(&c, 1_700_000_000); // now
    let receiver = Address::generate(&c.env);
    let sigs: Vec<BytesN<32>> = vec![&c.env];
    let null = c
        .client
        .authorize_receive(&c.token, &receiver, &dummy_proof(&c.env), &sigs);
    assert_eq!(null, BytesN::from_array(&c.env, &[0x99u8; 32]));
}

#[test]
fn rejects_when_reserves_stale() {
    let c = setup();
    put_attestation(&c, 1_600_000_000); // ~year old → stale
    let receiver = Address::generate(&c.env);
    let sigs: Vec<BytesN<32>> = vec![&c.env];
    let res = c
        .client
        .try_authorize_receive(&c.token, &receiver, &dummy_proof(&c.env), &sigs);
    assert!(res.is_err());
}

#[test]
fn rejects_when_no_reserves() {
    let c = setup();
    // no attestation set
    let receiver = Address::generate(&c.env);
    let sigs: Vec<BytesN<32>> = vec![&c.env];
    let res = c
        .client
        .try_authorize_receive(&c.token, &receiver, &dummy_proof(&c.env), &sigs);
    assert!(res.is_err());
}

#[test]
#[should_panic]
fn rejects_when_ineligible() {
    let c = setup();
    put_attestation(&c, 1_700_000_000);
    let me = MockEligClient::new(&c.env, &c.elig_id);
    me.set_ok(&false);
    let receiver = Address::generate(&c.env);
    let sigs: Vec<BytesN<32>> = vec![&c.env];
    // eligibility mock panics → cross-call traps → authorize aborts
    c.client
        .authorize_receive(&c.token, &receiver, &dummy_proof(&c.env), &sigs);
}
