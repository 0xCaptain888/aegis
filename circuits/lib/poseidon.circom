pragma circom 2.1.9;
// Re-export shim → resolves to circomlib's Poseidon implementation.
// circomlib is installed via `npm install` (see package.json) into node_modules/circomlib.
include "../../node_modules/circomlib/circuits/poseidon.circom";
