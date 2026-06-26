pragma circom 2.1.9;

include "../lib/poseidon.circom";
include "../lib/comparators.circom";

/*
 * Aegis — ZK Proof-of-Reserves
 * ----------------------------------------------------------------------------
 * Statement proven (in zero knowledge):
 *   "I (the RWA issuer) custody reserve balances across N accounts whose SUM is
 *    >= the on-chain token totalSupply, AND those balances hash to a commitment
 *    I have published — WITHOUT revealing any individual balance or account."
 *
 * Public inputs (visible to the on-chain verifier):
 *   - totalSupply        : circulating supply of the RWA token (from the token contract)
 *   - reservesCommitment  : Poseidon commitment the issuer published earlier
 *   - minCollateralBps    : required collateralization in basis points (e.g. 10000 = 100%)
 *
 * Private inputs (secret, never leave the prover):
 *   - balances[N]         : per-account reserve balances
 *   - salt                : blinding factor for the commitment
 *
 * What the circuit enforces:
 *   1. Poseidon(balances..., salt) == reservesCommitment   (binds proof to a published commitment)
 *   2. sum(balances) * 10000 >= totalSupply * minCollateralBps  (solvency / over-collateralization)
 *
 * Notes:
 *   - N is fixed at compile time. We use N=8 accounts by default; pad unused
 *     slots with 0. Increase N and recompile for more custody accounts.
 *   - All amounts are integers in the token's smallest unit (e.g. 7 decimals on Stellar).
 *   - We bound each balance and the supply to 64 bits to keep the range checks sound
 *     and the field arithmetic safe (BN254 field >> 2^128, so sums of 8x64-bit are safe).
 */

template ProofOfReserves(N) {
    // ---- Public ----
    signal input totalSupply;
    signal input reservesCommitment;
    signal input minCollateralBps;     // basis points, e.g. 10000 == 100%

    // ---- Private ----
    signal input balances[N];
    signal input salt;

    // ---- 1. Commitment binding ----
    // Poseidon supports up to 16 inputs in circomlib's implementation; with N=8
    // plus salt we use 9 inputs which is within range.
    component hasher = Poseidon(N + 1);
    for (var i = 0; i < N; i++) {
        hasher.inputs[i] <== balances[i];
    }
    hasher.inputs[N] <== salt;
    reservesCommitment === hasher.out;

    // ---- 2. Range-check every balance to 64 bits (prevents field-wrap cheating) ----
    component balBits[N];
    for (var i = 0; i < N; i++) {
        balBits[i] = Num2Bits(64);
        balBits[i].in <== balances[i];
    }

    // total supply also bounded to 64 bits
    component supplyBits = Num2Bits(64);
    supplyBits.in <== totalSupply;

    // minCollateralBps bounded to 32 bits (max ~4.29e9 bps == 429,496% — plenty)
    component bpsBits = Num2Bits(32);
    bpsBits.in <== minCollateralBps;

    // ---- 3. Sum reserves ----
    signal partial[N + 1];
    partial[0] <== 0;
    for (var i = 0; i < N; i++) {
        partial[i + 1] <== partial[i] + balances[i];
    }
    signal reserveSum;
    reserveSum <== partial[N];

    // ---- 4. Solvency check: reserveSum * 10000 >= totalSupply * minCollateralBps ----
    // Left and right are each <= 2^64 * 2^32 = 2^96, well within BN254 field.
    signal lhs;
    signal rhs;
    lhs <== reserveSum * 10000;
    rhs <== totalSupply * minCollateralBps;

    // GreaterEqThan over 100 bits (covers up to 2^96 comfortably)
    component ge = GreaterEqThan(100);
    ge.in[0] <== lhs;
    ge.in[1] <== rhs;
    ge.out === 1;   // proof fails to build / verify if under-collateralized
}

// Default deployment: 8 custody accounts.
component main { public [ totalSupply, reservesCommitment, minCollateralBps ] } = ProofOfReserves(8);
