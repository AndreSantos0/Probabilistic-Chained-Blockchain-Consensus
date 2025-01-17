use std::collections::HashSet;
use rand::{Rng, SeedableRng};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use sha2::{Sha256, Digest};


pub fn vrf_prove(private_key: &Ed25519KeyPair, seed: u64, sample_size: usize) -> (HashSet<u32>, Vec<u8>) {
    let seed_bytes = seed.to_le_bytes();
    let proof = private_key.sign(&seed_bytes);
    let mut hasher = Sha256::new();
    hasher.update(proof.as_ref());
    let hash_output = hasher.finalize();
    let mut rng = rand::rngs::StdRng::from_seed(hash_output[0..32].try_into().unwrap());
    let sample_set: HashSet<u32> = (0..sample_size).map(|_| rng.random()).collect();
    (sample_set, proof.as_ref().to_vec())
}

pub fn vrf_verify(
    public_key: &[u8],
    seed: u64,
    sample_size: usize,
    sample_set: HashSet<u32>,
    proof: &[u8],
) -> bool {
    let public_key = UnparsedPublicKey::new(&ED25519, public_key);
    let seed_bytes = seed.to_le_bytes();

    if public_key.verify(&seed_bytes, proof).is_err() {
        return false;
    }

    let mut hasher = Sha256::new();
    hasher.update(proof);
    let hash_output = hasher.finalize();
    let mut rng = rand::rngs::StdRng::from_seed(hash_output[0..32].try_into().unwrap());
    let expected_sample_set: HashSet<u32> = (0..sample_size).map(|_| rng.random()).collect();
    sample_set == expected_sample_set
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::KeyPair;

    #[test]
    fn test_vrf_prove_and_verify() {
        let rng = SystemRandom::new();
        let private_key = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let private_key = Ed25519KeyPair::from_pkcs8(private_key.as_ref()).unwrap();
        let public_key = private_key.public_key().as_ref().to_vec();

        let seed = 42u64;
        let sample_size = 5;

        let (sample_set, proof) = vrf_prove(&private_key, seed, sample_size);
        assert_eq!(sample_set.len(), sample_size);

        let is_valid = vrf_verify(&public_key, seed, sample_size, sample_set.clone(), &proof);
        assert!(is_valid);

        // Test with tampered proof
        let mut tampered_proof = proof.clone();
        tampered_proof[0] ^= 1;
        let is_valid = vrf_verify(&public_key, seed, sample_size, sample_set, &tampered_proof);
        assert!(!is_valid);
    }

    #[test]
    fn test_vrf_prove_different_seeds() {
        let rng = SystemRandom::new();
        let private_key = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let private_key = Ed25519KeyPair::from_pkcs8(private_key.as_ref()).unwrap();
        let public_key = private_key.public_key().as_ref().to_vec();

        let seed1 = 42u64;
        let seed2 = 43u64;
        let sample_size = 5;

        let (sample_set1, proof1) = vrf_prove(&private_key, seed1, sample_size);
        let (sample_set2, proof2) = vrf_prove(&private_key, seed2, sample_size);

        assert_ne!(sample_set1, sample_set2);
        assert_ne!(proof1, proof2);

        assert!(vrf_verify(&public_key, seed1, sample_size, sample_set1, &proof1));
        assert!(vrf_verify(&public_key, seed2, sample_size, sample_set2, &proof2));
    }
}
