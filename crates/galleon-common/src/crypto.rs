//! Cryptographic functions and hardware-accelerated encryption for GalleonFS

use aes_gcm::{Aes256Gcm, Key, Nonce, aead::Aead, KeyInit};
use blake3::Hasher as Blake3Hasher;
use ring::{digest, hmac, rand::{SecureRandom, SystemRandom}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::{Result, GalleonError};

/// Cryptographic engine for GalleonFS
pub struct CryptoEngine {
    rng: SystemRandom,
    keys: HashMap<String, Vec<u8>>,
    current_key_id: Option<String>,
}

impl CryptoEngine {
    /// Create a new cryptographic engine
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
            keys: HashMap::new(),
            current_key_id: None,
        }
    }

    /// Generate a new 256-bit encryption key
    pub fn generate_key(&self) -> Result<Vec<u8>> {
        let mut key = vec![0u8; 32]; // 256 bits
        self.rng.fill(&mut key)
            .map_err(|e| GalleonError::CryptoError(format!("Failed to generate key: {:?}", e)))?;
        Ok(key)
    }

    /// Add a new encryption key
    pub fn add_key(&mut self, key_id: String, key: Vec<u8>) -> Result<()> {
        if key.len() != 32 {
            return Err(GalleonError::CryptoError("Key must be 32 bytes (256 bits)".to_string()));
        }
        self.keys.insert(key_id.clone(), key);
        if self.current_key_id.is_none() {
            self.current_key_id = Some(key_id);
        }
        Ok(())
    }

    /// Set the current active key for encryption
    pub fn set_current_key(&mut self, key_id: String) -> Result<()> {
        if !self.keys.contains_key(&key_id) {
            return Err(GalleonError::CryptoError(format!("Key '{}' not found", key_id)));
        }
        self.current_key_id = Some(key_id);
        Ok(())
    }

    /// Encrypt data using AES-256-GCM
    pub fn encrypt(&self, data: &[u8]) -> Result<EncryptedData> {
        let key_id = self.current_key_id.as_ref()
            .ok_or_else(|| GalleonError::CryptoError("No encryption key set".to_string()))?;
        
        let key_bytes = self.keys.get(key_id)
            .ok_or_else(|| GalleonError::CryptoError(format!("Key '{}' not found", key_id)))?;

        let key = Key::from_slice(key_bytes);
        let cipher = Aes256Gcm::from_key(key);

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        self.rng.fill(&mut nonce_bytes)
            .map_err(|e| GalleonError::CryptoError(format!("Failed to generate nonce: {:?}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, data)
            .map_err(|e| GalleonError::CryptoError(format!("Encryption failed: {:?}", e)))?;

        Ok(EncryptedData {
            key_id: key_id.clone(),
            nonce: nonce_bytes.to_vec(),
            ciphertext,
            algorithm: "AES-256-GCM".to_string(),
        })
    }

    /// Decrypt data using AES-256-GCM
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>> {
        let key_bytes = self.keys.get(&encrypted.key_id)
            .ok_or_else(|| GalleonError::CryptoError(format!("Key '{}' not found", encrypted.key_id)))?;

        let key = Key::from_slice(key_bytes);
        let cipher = Aes256Gcm::from_key(key);
        let nonce = Nonce::from_slice(&encrypted.nonce);

        let plaintext = cipher.decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|e| GalleonError::CryptoError(format!("Decryption failed: {:?}", e)))?;

        Ok(plaintext)
    }

    /// Generate HMAC for data integrity
    pub fn generate_hmac(&self, data: &[u8]) -> Result<Vec<u8>> {
        let key_id = self.current_key_id.as_ref()
            .ok_or_else(|| GalleonError::CryptoError("No HMAC key set".to_string()))?;
        
        let key_bytes = self.keys.get(key_id)
            .ok_or_else(|| GalleonError::CryptoError(format!("Key '{}' not found", key_id)))?;

        let key = hmac::Key::new(hmac::HMAC_SHA256, key_bytes);
        let tag = hmac::sign(&key, data);
        Ok(tag.as_ref().to_vec())
    }

    /// Verify HMAC for data integrity
    pub fn verify_hmac(&self, data: &[u8], expected_hmac: &[u8], key_id: &str) -> Result<bool> {
        let key_bytes = self.keys.get(key_id)
            .ok_or_else(|| GalleonError::CryptoError(format!("Key '{}' not found", key_id)))?;

        let key = hmac::Key::new(hmac::HMAC_SHA256, key_bytes);
        match hmac::verify(&key, data, expected_hmac) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Encrypted data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub algorithm: String,
}

/// Hardware-accelerated hash functions
pub struct HashEngine;

impl HashEngine {
    /// Compute BLAKE3 hash (hardware-accelerated when available)
    pub fn blake3(data: &[u8]) -> Vec<u8> {
        let mut hasher = Blake3Hasher::new();
        hasher.update(data);
        hasher.finalize().as_bytes().to_vec()
    }

    /// Compute SHA-256 hash using hardware acceleration
    pub fn sha256(data: &[u8]) -> Vec<u8> {
        digest::digest(&digest::SHA256, data).as_ref().to_vec()
    }

    /// Compute SHA-512 hash using hardware acceleration
    pub fn sha512(data: &[u8]) -> Vec<u8> {
        digest::digest(&digest::SHA512, data).as_ref().to_vec()
    }

    /// Fast CRC32C checksum for data integrity
    pub fn crc32c(data: &[u8]) -> u32 {
        crc32fast::hash(data)
    }

    /// xxHash for non-cryptographic hashing
    pub fn xxhash64(data: &[u8]) -> u64 {
        xxhash_rust::xxh64::xxh64(data, 0)
    }
}

/// Key derivation functions
pub struct KeyDerivation;

impl KeyDerivation {
    /// Derive key using PBKDF2 with SHA-256
    pub fn pbkdf2_sha256(
        password: &[u8],
        salt: &[u8],
        iterations: u32,
        output_len: usize,
    ) -> Result<Vec<u8>> {
        use ring::pbkdf2;
        
        let mut output = vec![0u8; output_len];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            std::num::NonZeroU32::new(iterations).unwrap(),
            salt,
            password,
            &mut output,
        );
        Ok(output)
    }

    /// Generate cryptographically secure random salt
    pub fn generate_salt(length: usize) -> Result<Vec<u8>> {
        let rng = SystemRandom::new();
        let mut salt = vec![0u8; length];
        rng.fill(&mut salt)
            .map_err(|e| GalleonError::CryptoError(format!("Failed to generate salt: {:?}", e)))?;
        Ok(salt)
    }
}

/// Digital signature operations
pub struct DigitalSignature {
    rng: SystemRandom,
}

impl DigitalSignature {
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }

    /// Generate Ed25519 key pair (placeholder implementation)
    pub fn generate_ed25519_keypair(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        // Placeholder implementation - Ed25519 support will be completed in next iteration
        Ok((vec![0; 32], vec![0; 32]))
    }

    /// Sign data with Ed25519 (placeholder implementation)
    pub fn sign_ed25519(&self, _data: &[u8], _secret_key: &[u8]) -> Result<Vec<u8>> {
        // Placeholder implementation - Ed25519 support will be completed in next iteration
        Ok(vec![0; 64])
    }

    /// Verify Ed25519 signature (placeholder implementation)
    pub fn verify_ed25519(&self, _data: &[u8], _signature: &[u8], _public_key: &[u8]) -> Result<bool> {
        // Placeholder implementation - Ed25519 support will be completed in next iteration
        Ok(true)
    }
}

/// Hardware acceleration detection
pub struct HardwareAcceleration;

impl HardwareAcceleration {
    /// Check if AES-NI is available
    pub fn aes_ni_available() -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            is_x86_feature_detected!("aes")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }

    /// Check if AVX2 is available for SIMD operations
    pub fn avx2_available() -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            is_x86_feature_detected!("avx2")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }

    /// Check if SHA extensions are available
    pub fn sha_extensions_available() -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            is_x86_feature_detected!("sha")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }

    /// Get a summary of available hardware acceleration
    pub fn summary() -> HardwareAccelerationSummary {
        HardwareAccelerationSummary {
            aes_ni: Self::aes_ni_available(),
            avx2: Self::avx2_available(),
            sha_extensions: Self::sha_extensions_available(),
        }
    }
}

/// Summary of available hardware acceleration features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareAccelerationSummary {
    pub aes_ni: bool,
    pub avx2: bool,
    pub sha_extensions: bool,
}

impl Default for CryptoEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DigitalSignature {
    fn default() -> Self {
        Self::new()
    }
}