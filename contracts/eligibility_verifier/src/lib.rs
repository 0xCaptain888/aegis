#![no_std]
//! Aegis Investor Eligibility Verifier (Soroban contract)
//!
//! Verifies a Groth16 proof that an investor holds an issuer-signed credential
//! satisfying a gate's policy (min KYC level, accreditation, allowlisted
//! jurisdiction, not expired) — revealing ONLY a boolean + a nullifier.
//!
//! The nullifier prevents one credential from being used twice for the SAME
//! gated action, without linking uses to an identity.
//!
//! Public signals order (must match circuit `Eligibility`):
//!   [0] issuerPubKeyX
//!   [1] issuerPubKeyY
//!   [2] requiredKycLevel
//!   [3] requireAccredited
//!   [4] allowedJurisdictionRoot
//!   [5] currentTimestamp
//!   [6] actionId
//!   [7] nullifier

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
    UnknownGate = 3,
    PolicyMismatch = 4,
    ProofInvalid = 5,
    NullifierUsed = 6,
    Expired = 7,
    SignalCount = 8,
}

/// A gate's policy. The verifier checks the proof's PUBLIC signals equal these,
/// so a proof generated for a different policy cannot be replayed here.
#[contracttype]
#[derive(Clone)]
pub struct GatePolicy {
    pub issuer_pubkey_x: BytesN<32>,
    pub issuer_pubkey_y: BytesN<32>,
    pub required_kyc_level: u32,
    pub require_accredited: bool,
    pub allowed_jurisdiction_root: BytesN<32>,
    pub action_id: BytesN<32>,
    pub vk_id: u32,
    pub max_skew_secs: u64, // tolerance between proof timestamp & ledger time
}

#[contracttype]
pub enum DataKey {
    Admin,
    Verifier,
    Gate(BytesN<32>),                 // gate_id -> GatePolicy
    Nullifier(BytesN<32>, BytesN<32>), // (gate_id, nullifier) -> ()
}

#[contract]
pub struct EligibilityVerifier;

#[contractimpl]
impl EligibilityVerifier {
    pub fn init(env: Env, admin: Address, verifier: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Verifier, &verifier);
        Ok(())
    }

    /// Admin/issuer registers a gate policy under a chosen gate_id.
    pub fn set_gate(env: Env, admin: Address, gate_id: BytesN<32>, policy: GatePolicy) -> Result<(), Error> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        stored.require_auth();
        let _ = admin; // explicit caller for clarity
        env.storage().persistent().set(&DataKey::Gate(gate_id.clone()), &policy);
        env.events()
            .publish((Symbol::new(&env, "gate_set"), gate_id), policy.vk_id);
        Ok(())
    }

    /// Verify eligibility. On success, records the nullifier so it can't be reused
    /// for this gate, and returns the nullifier that was consumed.
    pub fn verify_eligibility(
        env: Env,
        gate_id: BytesN<32>,
        proof: Groth16Proof,
        signals: PublicSignals,
    ) -> Result<BytesN<32>, Error> {
        if signals.len() != 8 {
            return Err(Error::SignalCount);
        }
        let policy: GatePolicy = env
            .storage()
            .persistent()
            .get(&DataKey::Gate(gate_id.clone()))
            .ok_or(Error::UnknownGate)?;
        let verifier: Address = env
            .storage()
            .instance()
            .get(&DataKey::Verifier)
            .ok_or(Error::NotInitialized)?;

        // ---- bind every policy-controlled public signal ----
        if signals.get(0).unwrap() != policy.issuer_pubkey_x {
            return Err(Error::PolicyMismatch);
        }
        if signals.get(1).unwrap() != policy.issuer_pubkey_y {
            return Err(Error::PolicyMismatch);
        }
        if be32_to_u32(&signals.get(2).unwrap()) != policy.required_kyc_level {
            return Err(Error::PolicyMismatch);
        }
        let req_acc = be32_to_u32(&signals.get(3).unwrap()) != 0;
        if req_acc != policy.require_accredited {
            return Err(Error::PolicyMismatch);
        }
        if signals.get(4).unwrap() != policy.allowed_jurisdiction_root {
            return Err(Error::PolicyMismatch);
        }
        if signals.get(6).unwrap() != policy.action_id {
            return Err(Error::PolicyMismatch);
        }

        // ---- freshness: proof timestamp must be within skew of ledger time ----
        let proof_ts = be32_to_u64(&signals.get(5).unwrap());
        let now = env.ledger().timestamp();
        let lo = now.saturating_sub(policy.max_skew_secs);
        let hi = now.saturating_add(policy.max_skew_secs);
        if proof_ts < lo || proof_ts > hi {
            return Err(Error::Expired);
        }

        // ---- nullifier must be unused for this gate ----
        let nullifier = signals.get(7).unwrap();
        let nk = DataKey::Nullifier(gate_id.clone(), nullifier.clone());
        if env.storage().persistent().has(&nk) {
            return Err(Error::NullifierUsed);
        }

        // ---- verify the SNARK ----
        if !verify_via_contract(&env, &verifier, policy.vk_id, &proof, &signals) {
            return Err(Error::ProofInvalid);
        }

        // consume the nullifier
        env.storage().persistent().set(&nk, &true);
        env.events().publish(
            (Symbol::new(&env, "eligible"), gate_id),
            nullifier.clone(),
        );
        Ok(nullifier)
    }

    pub fn is_nullifier_used(env: Env, gate_id: BytesN<32>, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(gate_id, nullifier))
    }

    pub fn gate(env: Env, gate_id: BytesN<32>) -> Option<GatePolicy> {
        env.storage().persistent().get(&DataKey::Gate(gate_id))
    }
}

fn be32_to_u32(b: &BytesN<32>) -> u32 {
    let a = b.to_array();
    ((a[28] as u32) << 24) | ((a[29] as u32) << 16) | ((a[30] as u32) << 8) | (a[31] as u32)
}

fn be32_to_u64(b: &BytesN<32>) -> u64 {
    let a = b.to_array();
    let mut v: u64 = 0;
    for i in 24..32 {
        v = (v << 8) | (a[i] as u64);
    }
    v
}

#[cfg(test)]
mod test;
