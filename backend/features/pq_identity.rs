// ─────────────────────────────────────────────────────────────
// Feature 5: Post-Quantum Stateless Identity
// ─────────────────────────────────────────────────────────────
// No passwords — ephemeral Dilithium keys with auto-rotation.

use crate::crypto::hash::{hex_encode, sha3_256_hex};
use crate::crypto::key_evolution::KeyEvolution;
use crate::crypto::params::PQCParams;
use crate::crypto::signatures::PQSignature;
use crate::error::{AstraError, AstraResult};
use serde::Serialize;
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize)]
pub struct IdentityProof {
    pub identity_id: String,
    pub public_key_hash: String,
    pub epoch: u64,
    pub created_at: i64,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthResponse {
    pub authenticated: bool,
    pub identity_id: String,
    pub signature_valid: bool,
    pub epoch: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyRotationResult {
    pub old_pk_hash: String,
    pub new_pk_hash: String,
    pub new_epoch: u64,
    pub forward_secure: bool,
}

pub struct PQIdentity {
    sig: PQSignature,
    key_evo: KeyEvolution,
    current_pk: Vec<u8>,
    current_sk: Vec<u8>,
    identity_id: String,
    rotation_count: u64,
}

impl Drop for PQIdentity {
    fn drop(&mut self) {
        self.current_sk.zeroize();
    }
}

pub struct PQIdentitySigner {
    sig: PQSignature,
    secret_key: Vec<u8>,
}

impl Drop for PQIdentitySigner {
    fn drop(&mut self) {
        self.secret_key.zeroize();
    }
}

impl PQIdentitySigner {
    pub fn sign_message_hex(&self, message: &[u8]) -> AstraResult<String> {
        let signature = self.sig.sign(&self.secret_key, message)?;
        Ok(hex_encode(&signature))
    }
}

impl PQIdentity {
    pub fn new() -> Self {
        let params = PQCParams::default();
        let sig = PQSignature::new(params.clone());
        let key_evo = KeyEvolution::new(params);
        Self {
            sig,
            key_evo,
            current_pk: Vec::new(),
            current_sk: Vec::new(),
            identity_id: String::new(),
            rotation_count: 0,
        }
    }

    /// Create a new post-quantum identity.
    pub fn create_identity(&mut self) -> IdentityProof {
        let seed = format!("identity:{}", chrono::Utc::now().timestamp_millis());
        let (pk, sk) = self.sig.keygen(Some(seed.as_bytes()));
        let pk_hash = sha3_256_hex(&pk)[..16].to_string();
        self.identity_id = sha3_256_hex(format!("id:{}", pk_hash).as_bytes())[..12].to_string();
        self.current_pk = pk;
        self.current_sk = sk;

        self.key_evo.register_validator(
            &self.identity_id,
            self.current_sk.clone(),
            self.current_pk.clone(),
        );

        IdentityProof {
            identity_id: self.identity_id.clone(),
            public_key_hash: pk_hash,
            epoch: self.key_evo.current_epoch(),
            created_at: chrono::Utc::now().timestamp_millis(),
            algorithm: "Dilithium-Astra".into(),
        }
    }

    /// Authenticate with a PQ proof.
    pub fn authenticate(&self, challenge: &[u8]) -> AuthResponse {
        if self.current_sk.is_empty() {
            return AuthResponse {
                authenticated: false,
                identity_id: self.identity_id.clone(),
                signature_valid: false,
                epoch: 0,
            };
        }
        let sig_result = self.sig.sign(&self.current_sk, challenge);
        let valid = sig_result
            .as_ref()
            .map(|s| self.sig.verify(&self.current_pk, challenge, s))
            .unwrap_or(false);
        AuthResponse {
            authenticated: valid,
            identity_id: self.identity_id.clone(),
            signature_valid: valid,
            epoch: self.key_evo.current_epoch(),
        }
    }

    /// Rotate keys forward-securely.
    pub fn rotate_keys(&mut self) -> KeyRotationResult {
        let old_hash = sha3_256_hex(&self.current_pk)[..16].to_string();
        self.rotation_count += 1;

        // Evolve keys
        let new_epoch = (self.rotation_count + 1) * 100; // Trigger new epoch
        self.key_evo.evolve_keys(new_epoch);

        // Generate fresh keypair
        let seed = format!(
            "rotation:{}:{}",
            self.rotation_count,
            chrono::Utc::now().timestamp_millis()
        );
        let (pk, sk) = self.sig.keygen(Some(seed.as_bytes()));
        let new_hash = sha3_256_hex(&pk)[..16].to_string();
        self.current_pk = pk;
        self.current_sk = sk;

        KeyRotationResult {
            old_pk_hash: old_hash,
            new_pk_hash: new_hash,
            new_epoch: self.key_evo.current_epoch(),
            forward_secure: true,
        }
    }

    /// Get current identity proof.
    pub fn get_identity_proof(&self) -> IdentityProof {
        IdentityProof {
            identity_id: self.identity_id.clone(),
            public_key_hash: sha3_256_hex(&self.current_pk)[..16].to_string(),
            epoch: self.key_evo.current_epoch(),
            created_at: chrono::Utc::now().timestamp_millis(),
            algorithm: "Dilithium-Astra".into(),
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "identity_id": self.identity_id,
            "rotations": self.rotation_count,
            "epoch": self.key_evo.current_epoch(),
            "has_keys": !self.current_pk.is_empty(),
        })
    }

    pub fn ensure_identity(&mut self) -> IdentityProof {
        if self.current_pk.is_empty() || self.current_sk.is_empty() {
            return self.create_identity();
        }
        self.get_identity_proof()
    }

    pub fn public_key_hex(&self) -> Option<String> {
        (!self.current_pk.is_empty()).then(|| hex_encode(&self.current_pk))
    }

    pub fn public_key_hash_full(&self) -> Option<String> {
        (!self.current_pk.is_empty()).then(|| sha3_256_hex(&self.current_pk)[..32].to_string())
    }

    /// Returns true if a secret key is loaded (never exposes the raw key material).
    pub fn has_secret_key(&self) -> bool {
        !self.current_sk.is_empty()
    }

    pub fn detached_signer(&self) -> Option<PQIdentitySigner> {
        (!self.current_sk.is_empty()).then(|| PQIdentitySigner {
            sig: self.sig.clone(),
            secret_key: self.current_sk.clone(),
        })
    }

    pub fn sign_message_hex(&self, message: &[u8]) -> AstraResult<String> {
        let signer = self.detached_signer().ok_or(AstraError::AuthRequired)?;
        signer.sign_message_hex(message)
    }
}
