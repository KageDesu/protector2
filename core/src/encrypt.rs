//! Level 3: AES-256-GCM encryption of the merged bundle + a tiny runtime loader.
//!
//! The working merged bundle (identical to what level 2 produces and runs) is
//! encrypted with a random 256-bit key and 96-bit nonce. The master plugin file
//! is replaced by a small loader that, at runtime inside NW.js, decrypts the
//! blob with Node's built-in `crypto` and executes it via indirect `eval` in
//! global scope — so the executed code is byte-identical to the working bundle.
//!
//! Honest limitation: the key necessarily ships inside the game, so this raises
//! the bar against casual copying (the code is no longer plaintext you can open
//! and read), not against a determined reverse-engineer. It targets NW.js
//! desktop builds (Game.exe), where `require('crypto')` is available.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub struct Encrypted {
    pub key: [u8; 32],
    pub nonce: [u8; 12],
    pub tag: [u8; 16],
    pub ciphertext: Vec<u8>,
}

/// Encrypt `plain` with a freshly generated random key + nonce.
pub fn encrypt(plain: &[u8]) -> Result<Encrypted, String> {
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut key).map_err(|e| format!("rng error: {e}"))?;
    getrandom::fill(&mut nonce).map_err(|e| format!("rng error: {e}"))?;

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    // AES-GCM returns ciphertext with the 16-byte auth tag appended.
    let mut ct = cipher
        .encrypt(&nonce.into(), plain)
        .map_err(|e| format!("encryption failed: {e}"))?;

    if ct.len() < 16 {
        return Err("ciphertext too short".into());
    }
    let tag_bytes = ct.split_off(ct.len() - 16);
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&tag_bytes);

    Ok(Encrypted {
        key,
        nonce,
        tag,
        ciphertext: ct,
    })
}

/// Build the self-contained loader JS that decrypts and runs the bundle at
/// runtime. Everything (key, nonce, tag, ciphertext) is embedded as base64.
pub fn build_loader(enc: &Encrypted) -> String {
    let k = STANDARD.encode(enc.key);
    let n = STANDARD.encode(enc.nonce);
    let t = STANDARD.encode(enc.tag);
    let d = STANDARD.encode(&enc.ciphertext);

    // Indirect eval `(0,eval)(s)` runs in global scope → top-level vars and
    // implicit globals behave exactly as in the plain bundle. Not strict.
    format!(
        "/* Protected by PKD Protector */\n\
         (function(){{\n\
         var _k=\"{k}\",_n=\"{n}\",_t=\"{t}\",_d=\"{d}\";\n\
         try{{\n\
         var c=require('crypto');\n\
         var dc=c.createDecipheriv('aes-256-gcm',Buffer.from(_k,'base64'),Buffer.from(_n,'base64'));\n\
         dc.setAuthTag(Buffer.from(_t,'base64'));\n\
         var s=Buffer.concat([dc.update(Buffer.from(_d,'base64')),dc.final()]).toString('utf8');\n\
         (0,eval)(s);\n\
         }}catch(e){{console.error('[PKD Protector] loader failed:',e);throw e;}}\n\
         }})();\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_produces_distinct_parts() {
        let enc = encrypt(b"console.log('hello world');").unwrap();
        assert_eq!(enc.key.len(), 32);
        assert_eq!(enc.nonce.len(), 12);
        assert_eq!(enc.tag.len(), 16);
        assert!(!enc.ciphertext.is_empty());
    }

    #[test]
    fn loader_embeds_all_parts_and_calls_crypto() {
        let enc = encrypt(b"var x=1;").unwrap();
        let loader = build_loader(&enc);
        assert!(loader.contains("createDecipheriv('aes-256-gcm'"));
        assert!(loader.contains("setAuthTag"));
        assert!(loader.contains("(0,eval)(s)"));
        assert!(loader.contains(&STANDARD.encode(enc.key)));
    }

    #[test]
    fn round_trip_in_rust() {
        // Decrypt with the same crate to prove the parts are consistent.
        use aes_gcm::aead::Aead;
        let plain = b"The quick brown fox jumps over 13 lazy dogs. getSelfSwitchValue=1;";
        let enc = encrypt(plain).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&enc.key).unwrap();
        let mut combined = enc.ciphertext.clone();
        combined.extend_from_slice(&enc.tag);
        let out = cipher.decrypt(&enc.nonce.into(), combined.as_ref()).unwrap();
        assert_eq!(out, plain);
    }
}
