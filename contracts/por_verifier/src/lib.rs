#![no_std]
//! Aegis Proof-of-Reserves Verifier (Soroban contract)
//!
//! Stores, per RWA token, the issuer's published `reserves_commitment` and the
//! required collateralization in basis points. Accepts a Groth16 proof that the
//! issuer's (secret) reserve balances hash to that commitment AND that the
//! reserves over-collateralize the token's circulating supply — all WITHOUT
//! revealing any individual balance.
//!
//! Public signals order (must match the circuit `ProofOfReserves`):
//!   [0] totalSupply
//!   [1] reservesCommitment
//!   [2] minCollateralBps
//!
//! After a successful verification, we record a timestamped "attestation" anyone
//! can read on-chain — turning a manual monthly PoR report into a live, verifiable
//! on-chain fact.

mod groth16;

use groth16::{verify_via_contract, Groth16Proof, PublicSignals};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    CommitmentMismatch = 4,
    SupplyMismatch = 5,
    ProofInvalid = 6,
    PolicyMismatch = 7,
}

#[contracttype]
#[derive(Clone)]
pub struct ReservePolicy {
    pub issuer: Address,
    pub reserves_commitment: BytesN<32>,
    pub min_collateral_bps: u32,
    pub vk_id: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct Attestation {
    pub token: Address,
    pub total_supply: i128,
    pub reserves_commitment: BytesN<32>,
    pub min_collateral_bps: u32,
    pub verified_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Verifier,
    Policy(Address),       // token -> ReservePolicy
    LastAttestation(Address), // token -> Attestation
}

#[contract]
pub struct PorVerifier;

#[contractimpl]
impl PorVerifier {
    /// One-time init. `verifier` is the deployed groth16_verifier contract.
    pub fn init(env: Env, admin: Address, verifier: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Verifier, &verifier);
        Ok(())
    }

    /// Issuer registers (or updates) the PoR policy for one of their RWA tokens.
    pub fn set_policy(
        env: Env,
        token: Address,
        issuer: Address,
        reserves_commitment: BytesN<32>,
        min_collateral_bps: u32,
        vk_id: u32,
    ) -> Result<(), Error> {
        issuer.require_auth();
        let policy = ReservePolicy {
            issuer,
            reserves_commitment,
            min_collateral_bps,
            vk_id,
        };
        env.storage().persistent().set(&DataKey::Policy(token.clone()), &policy);
        env.events().publish(
            (Symbol::new(&env, "policy_set"), token.clone()),
            (policy.reserves_commitment.clone(), policy.min_collateral_bps),
        );
        Ok(())
    }

    /// Anyone can submit a fresh proof to refresh the on-chain attestation.
    /// `claimed_supply` must equal the proof's public totalSupply signal AND the
    /// commitment / bps signals must equal the registered policy — so a stale or
    /// swapped proof cannot pass.
    pub fn attest(
        env: Env,
        token: Address,
        claimed_supply: i128,
        proof: Groth16Proof,
        signals: PublicSignals,
    ) -> Result<Attestation, Error> {
        let policy: ReservePolicy = env
            .storage()
            .persistent()
            .get(&DataKey::Policy(token.clone()))
            .ok_or(Error::NotInitialized)?;
        let verifier: Address = env
            .storage()
            .instance()
            .get(&DataKey::Verifier)
            .ok_or(Error::NotInitialized)?;

        // signals must be exactly [totalSupply, reservesCommitment, minCollateralBps]
        if signals.len() != 3 {
            return Err(Error::PolicyMismatch);
        }

        // bind public signal[1] to the registered commitment
        let sig_commitment = signals.get(1).unwrap();
        if sig_commitment != policy.reserves_commitment {
            return Err(Error::CommitmentMismatch);
        }

        // bind public signal[2] to the registered bps
        let sig_bps = be32_to_u32(&signals.get(2).unwrap());
        if sig_bps != policy.min_collateral_bps {
            return Err(Error::PolicyMismatch);
        }

        // bind public signal[0] to the claimed supply
        let sig_supply = be32_to_i128(&signals.get(0).unwrap());
        if sig_supply != claimed_supply {
            return Err(Error::SupplyMismatch);
        }

        // verify the SNARK
        if !verify_via_contract(&env, &verifier, policy.vk_id, &proof, &signals) {
            return Err(Error::ProofInvalid);
        }

        let attestation = Attestation {
            token: token.clone(),
            total_supply: claimed_supply,
            reserves_commitment: policy.reserves_commitment.clone(),
            min_collateral_bps: policy.min_collateral_bps,
            verified_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::LastAttestation(token.clone()), &attestation);
        env.events().publish(
            (Symbol::new(&env, "attested"), token.clone()),
            (claimed_supply, attestation.verified_at),
        );
        Ok(attestation)
    }

    /// Read the most recent verified attestation for a token (or None).
    pub fn last_attestation(env: Env, token: Address) -> Option<Attestation> {
        env.storage()
            .persistent()
            .get(&DataKey::LastAttestation(token))
    }

    pub fn policy(env: Env, token: Address) -> Option<ReservePolicy> {
        env.storage().persistent().get(&DataKey::Policy(token))
    }
}

// ---- helpers: decode 32-byte big-endian field elements into native ints ----

fn be32_to_u32(b: &BytesN<32>) -> u32 {
    let arr = b.to_array();
    // value lives in the low 4 bytes for small ints
    ((arr[28] as u32) << 24)
        | ((arr[29] as u32) << 16)
        | ((arr[30] as u32) << 8)
        | (arr[31] as u32)
}

fn be32_to_i128(b: &BytesN<32>) -> i128 {
    let arr = b.to_array();
    let mut v: i128 = 0;
    // low 16 bytes -> i128 (supply fits in 64 bits per the circuit, so safe)
    for i in 16..32 {
        v = (v << 8) | (arr[i] as i128);
    }
    v
}

#[cfg(test)]
mod test;
