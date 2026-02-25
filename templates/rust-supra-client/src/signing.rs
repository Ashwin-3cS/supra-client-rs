//! Ed25519 key management and BCS transaction signing.
//!
//! For MVP phase this module provides key generation and address derivation.
//! Full transaction signing (BCS payload) is used for submit_transaction.

use anyhow::{Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha3::{Digest, Sha3_256};

use crate::types::{
    AccountAddress, Ed25519PublicKey, Ed25519Signature, RawTransaction, SignedTransaction,
    TransactionAuthenticator,
};

// ─── Constants ────────────────────────────────────────────────────────────────
const DOMAIN_SEPARATOR: &str = "SUPRA::RawTransaction";

// ─── Keypair ──────────────────────────────────────────────────────────────────

/// Ed25519 keypair for signing Supra transactions.
pub struct Keypair {
    inner: SigningKey,
}

impl Keypair {
    /// Generate a fresh random keypair (uses OS entropy).
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { inner: signing_key }
    }

    /// Load from a 64-hex-char private key string (or 32-byte seed hex).
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str.trim_start_matches("0x"))
            .context("Private key must be valid hex")?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Private key must be exactly 32 bytes (64 hex chars)"))?;
        Ok(Self {
            inner: SigningKey::from_bytes(&arr),
        })
    }

    /// Load from the `SUPRA_PRIVATE_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let hex =
            std::env::var("SUPRA_PRIVATE_KEY").context("SUPRA_PRIVATE_KEY env var not set")?;
        Self::from_hex(&hex)
    }

    /// Private key bytes as hex.
    pub fn private_hex(&self) -> String {
        hex::encode(self.inner.to_bytes())
    }

    /// Public key bytes as hex.
    pub fn public_hex(&self) -> String {
        hex::encode(self.inner.verifying_key().to_bytes())
    }

    /// Derive the corresponding account address from this keypair's public key.
    ///
    /// Supra uses SHA3-256(pubkey || 0x00) truncated to 32 bytes, same as Aptos.
    pub fn address(&self) -> AccountAddress {
        let pubkey = self.inner.verifying_key().to_bytes();
        let mut hasher = Sha3_256::new();
        hasher.update(pubkey);
        hasher.update([0x00u8]); // single-signer scheme byte
        let hash = hasher.finalize();
        AccountAddress(format!("0x{}", hex::encode(hash)))
    }

    /// Sign an arbitrary byte slice.
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.inner.sign(msg).to_bytes().to_vec()
    }

    /// Sign a RawTransaction using the standard Supra/Aptos DOMAIN_SEPARATOR.
    pub fn sign_transaction(&self, raw: &RawTransaction) -> Result<SignedTransaction> {
        // 1. BCS serialize the RawTx
        let raw_bytes = bcs::to_bytes(raw).context("Failed to BCS serialize RawTransaction")?;

        // 2. Hash the Domain Separator
        let mut hasher = Sha3_256::new();
        hasher.update(DOMAIN_SEPARATOR.as_bytes());
        let domain_hash = hasher.finalize();

        // 3. Prepend the hashed domain to the message bytes
        let mut msg_to_sign = Vec::with_capacity(32 + raw_bytes.len());
        msg_to_sign.extend_from_slice(&domain_hash);
        msg_to_sign.extend_from_slice(&raw_bytes);

        // 4. Sign the payload
        let signature_bytes = self.inner.sign(&msg_to_sign).to_bytes();

        let pub_key_bytes = self.inner.verifying_key().to_bytes();

        // 5. Construct the SignedTransaction object
        Ok(SignedTransaction {
            raw_txn: raw.clone(),
            authenticator: TransactionAuthenticator::Ed25519 {
                public_key: Ed25519PublicKey(pub_key_bytes),
                signature: Ed25519Signature(signature_bytes),
            },
        })
    }
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keypair")
            .field("public_key", &self.public_hex())
            .field("address", &self.address().to_string())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_is_non_deterministic() {
        let a = Keypair::generate();
        let b = Keypair::generate();
        assert_ne!(a.private_hex(), b.private_hex());
    }

    #[test]
    fn test_roundtrip_hex() {
        let kp = Keypair::generate();
        let priv_hex = kp.private_hex();
        let kp2 = Keypair::from_hex(&priv_hex).unwrap();
        assert_eq!(kp.public_hex(), kp2.public_hex());
    }

    #[test]
    fn test_address_is_32_bytes() {
        let kp = Keypair::generate();
        let addr = kp.address();
        // "0x" + 64 hex chars = 66 chars total
        let hex_part = addr.0.trim_start_matches("0x");
        assert_eq!(
            hex_part.len(),
            64,
            "address must be 32 bytes = 64 hex chars"
        );
    }

    #[test]
    fn test_sign_deterministic() {
        let kp = Keypair::generate();
        let msg = b"hello supra";
        let sig1 = kp.sign(msg);
        let sig2 = kp.sign(msg);
        assert_eq!(sig1, sig2);
    }
}
