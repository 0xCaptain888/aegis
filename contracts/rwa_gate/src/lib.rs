#![no_std]
//! Aegis RWA Gate (Soroban contract)
//!
//! The orchestration layer that makes the two ZK proofs *load-bearing for real
//! money*: a holder may only RECEIVE a gated RWA token if
//!   (1) the token's Proof-of-Reserves attestation is fresh (reserves cover
//!       supply), AND
//!   (2) the receiver has proven eligibility (KYC/jurisdiction/accreditation)
//!       via a valid, unused eligibility proof for this gate.
//!
//! This contract does NOT re-implement verification; it composes the two
//! verifier contracts via cross-contract calls, then authorizes the transfer.
//! That keeps each concern in one place and mirrors how SDF's ASP model gates
//! movement of value behind compliance checks.

mod groth16;

use groth16::{Groth16Proof, PublicSignals};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, vec, Address, BytesN, Env, IntoVal,
    Symbol, Val, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NoReserves = 3,
    ReservesStale = 4,
    Undercollateralized = 5,
    EligibilityFailed = 6,
    NotConfigured = 7,
}

#[contracttype]
#[derive(Clone)]
pub struct GateConfig {
    pub token: Address,
    pub por_verifier: Address,
    pub eligibility_verifier: Address,
    pub eligibility_gate_id: BytesN<32>,
    pub max_reserve_age_secs: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Config(Address), // token -> GateConfig
}

// Minimal shape of the PoR attestation we read back from por_verifier.
#[contracttype]
#[derive(Clone)]
pub struct Attestation {
    pub token: Address,
    pub total_supply: i128,
    pub reserves_commitment: BytesN<32>,
    pub min_collateral_bps: u32,
    pub verified_at: u64,
}

#[contract]
pub struct RwaGate;

#[contractimpl]
impl RwaGate {
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn configure(env: Env, admin: Address, config: GateConfig) -> Result<(), Error> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        stored.require_auth();
        let _ = admin;
        env.storage()
            .persistent()
            .set(&DataKey::Config(config.token.clone()), &config);
        Ok(())
    }

    /// Check that reserves are fresh & sufficient for `token`. Reads the latest
    /// PoR attestation from the por_verifier and validates its age.
    pub fn check_reserves(env: Env, token: Address) -> Result<bool, Error> {
        let cfg: GateConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config(token.clone()))
            .ok_or(Error::NotConfigured)?;

        let att_args: Vec<Val> = vec![&env, token.clone().into_val(&env)];
        let att: Option<Attestation> = env.invoke_contract(
            &cfg.por_verifier,
            &Symbol::new(&env, "last_attestation"),
            att_args,
        );
        let att = att.ok_or(Error::NoReserves)?;

        let now = env.ledger().timestamp();
        if now.saturating_sub(att.verified_at) > cfg.max_reserve_age_secs {
            return Err(Error::ReservesStale);
        }
        Ok(true)
    }

    /// The compliant "may this receiver get this token?" gate. Verifies the
    /// receiver's eligibility proof (consuming its nullifier) AND confirms fresh
    /// reserves, then emits an authorization event the token/issuer can act on.
    ///
    /// Returns the consumed nullifier on success.
    pub fn authorize_receive(
        env: Env,
        token: Address,
        receiver: Address,
        eligibility_proof: Groth16Proof,
        eligibility_signals: Vec<BytesN<32>>,
    ) -> Result<BytesN<32>, Error> {
        let cfg: GateConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config(token.clone()))
            .ok_or(Error::NotConfigured)?;

        // (1) reserves fresh & sufficient
        Self::check_reserves(env.clone(), token.clone())?;

        // (2) receiver eligibility — cross-call into eligibility_verifier.
        // It returns the nullifier on success and panics/errs on failure, which
        // bubbles up and aborts the transfer authorization atomically.
        let elig_args: Vec<Val> = vec![
            &env,
            cfg.eligibility_gate_id.clone().into_val(&env),
            eligibility_proof.into_val(&env),
            eligibility_signals.into_val(&env),
        ];
        let nullifier: BytesN<32> = env.invoke_contract(
            &cfg.eligibility_verifier,
            &Symbol::new(&env, "verify_eligibility"),
            elig_args,
        );

        env.events().publish(
            (Symbol::new(&env, "authorized"), token.clone()),
            (receiver.clone(), nullifier.clone()),
        );
        Ok(nullifier)
    }

    pub fn config(env: Env, token: Address) -> Option<GateConfig> {
        env.storage().persistent().get(&DataKey::Config(token))
    }
}

#[cfg(test)]
mod test;
