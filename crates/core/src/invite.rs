//! Invitations.
//!
//! A full invitation carries the sender's public key (verified by the QUIC/TLS
//! handshake, so the receiver always knows it reached the right machine) plus a
//! random 128-bit access token. Only a BLAKE3 hash of the token is stored.
//!
//! A short code (e.g. `K7QM-9XRT-4HWD`, ~59 bits) is an optional convenience:
//! Argon2id stretches it into an Ed25519 key and an AEAD key. The sender
//! publishes the encrypted invitation under that derived key through the iroh
//! address-lookup service, and the receiver derives the same key to fetch and
//! decrypt it. The lookup service only ever sees ciphertext, the code expires,
//! and new devices still need the sender's approval by default, so the short
//! code is never the only secret protecting file access.

use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use data_encoding::{BASE32_NOPAD, BASE64URL_NOPAD};
use iroh::{EndpointId, RelayUrl, SecretKey};

use crate::util::random_bytes;

pub const INVITE_PREFIX: &str = "RL1-";
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const CODE_LEN: usize = 12;

#[derive(Clone, Debug)]
pub struct Invite {
    pub endpoint_id: EndpointId,
    pub token: [u8; 16],
    pub relay_url: Option<RelayUrl>,
}

impl Invite {
    pub fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(48 + 64);
        bytes.extend_from_slice(self.endpoint_id.as_bytes());
        bytes.extend_from_slice(&self.token);
        if let Some(url) = &self.relay_url {
            bytes.extend_from_slice(url.to_string().as_bytes());
        }
        format!("{INVITE_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    pub fn decode(s: &str) -> Result<Self> {
        let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let upper = cleaned.to_ascii_uppercase();
        let body = upper
            .strip_prefix(INVITE_PREFIX)
            .ok_or_else(|| anyhow!("This doesn't look like a Relayless invitation."))?;
        let bytes = BASE32_NOPAD
            .decode(body.as_bytes())
            .map_err(|_| anyhow!("The invitation is damaged. Copy it again from the sender."))?;
        if bytes.len() < 48 {
            bail!("The invitation is incomplete. Copy it again from the sender.");
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes[..32]);
        let endpoint_id = EndpointId::from_bytes(&id).context("invalid sender key in invitation")?;
        let mut token = [0u8; 16];
        token.copy_from_slice(&bytes[32..48]);
        let relay_url = if bytes.len() > 48 {
            std::str::from_utf8(&bytes[48..])
                .ok()
                .and_then(|s| RelayUrl::from_str(s).ok())
        } else {
            None
        };
        Ok(Invite { endpoint_id, token, relay_url })
    }

    pub fn token_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.token)
    }
}

pub fn new_token() -> [u8; 16] {
    random_bytes::<16>()
}

pub fn token_hash(token_hex: &str) -> String {
    blake3::hash(token_hex.as_bytes()).to_hex().to_string()
}

pub fn is_short_code(s: &str) -> bool {
    normalize_code(s).is_some_and(|c| c.len() == CODE_LEN)
        && !s.trim().to_ascii_uppercase().starts_with(INVITE_PREFIX)
}

fn normalize_code(s: &str) -> Option<String> {
    let c: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if c.len() == CODE_LEN && c.bytes().all(|b| CODE_ALPHABET.contains(&b)) {
        Some(c)
    } else {
        None
    }
}

pub fn new_short_code() -> String {
    let raw: String = (0..CODE_LEN)
        .map(|_| {
            let i = (rand::random::<u32>() as usize) % CODE_ALPHABET.len();
            CODE_ALPHABET[i] as char
        })
        .collect();
    format!("{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12])
}

/// Derived material for a short code: (lookup key, encryption key).
pub fn derive_code_keys(code: &str) -> Result<(SecretKey, [u8; 32])> {
    let norm = normalize_code(code).ok_or_else(|| anyhow!("That code isn't valid. Codes look like K7QM-9XRT-4HWD."))?;
    let params = argon2::Params::new(32 * 1024, 2, 1, Some(64)).map_err(|e| anyhow!("{e}"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 64];
    argon
        .hash_password_into(norm.as_bytes(), b"relayless-shortcode-v1", &mut out)
        .map_err(|e| anyhow!("{e}"))?;
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&out[..32]);
    let mut key = [0u8; 32];
    key.copy_from_slice(&out[32..]);
    Ok((SecretKey::from_bytes(&seed), key))
}

/// Encrypts the invitation (sender key + token) for publication under a code.
pub fn seal_for_code(invite: &Invite, key: &[u8; 32]) -> Result<String> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = random_bytes::<12>();
    let mut plain = Vec::with_capacity(48);
    plain.extend_from_slice(invite.endpoint_id.as_bytes());
    plain.extend_from_slice(&invite.token);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
        .map_err(|_| anyhow!("encryption failed"))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(format!("rl1:{}", BASE64URL_NOPAD.encode(&out)))
}

pub fn open_from_code(sealed: &str, key: &[u8; 32]) -> Result<Invite> {
    let body = sealed.strip_prefix("rl1:").ok_or_else(|| anyhow!("unexpected code record"))?;
    let raw = BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| anyhow!("corrupt code record"))?;
    if raw.len() < 12 + 16 {
        bail!("corrupt code record");
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let plain = cipher
        .decrypt(Nonce::from_slice(&raw[..12]), &raw[12..])
        .map_err(|_| anyhow!("That code doesn't match. Check it with the sender."))?;
    if plain.len() != 48 {
        bail!("corrupt code record");
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&plain[..32]);
    let mut token = [0u8; 16];
    token.copy_from_slice(&plain[32..]);
    Ok(Invite {
        endpoint_id: EndpointId::from_bytes(&id).context("invalid key")?,
        token,
        relay_url: None,
    })
}

pub fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    match qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .dark_color(svg::Color("#0e0f11"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_roundtrip() {
        let sk = SecretKey::generate();
        let inv = Invite {
            endpoint_id: sk.public(),
            token: new_token(),
            relay_url: Some(RelayUrl::from_str("https://euc1-1.relay.n0.iroh.link./").unwrap()),
        };
        let s = inv.encode();
        let back = Invite::decode(&format!("  {}\n", s.to_lowercase())).unwrap();
        assert_eq!(back.endpoint_id, inv.endpoint_id);
        assert_eq!(back.token, inv.token);
        assert_eq!(back.relay_url, inv.relay_url);
    }

    #[test]
    fn code_roundtrip() {
        let code = new_short_code();
        assert!(is_short_code(&code));
        assert!(is_short_code(&code.to_lowercase().replace('-', " ")));
        let (k1, e1) = derive_code_keys(&code).unwrap();
        let (k2, e2) = derive_code_keys(&code.to_lowercase()).unwrap();
        assert_eq!(k1.public(), k2.public());
        assert_eq!(e1, e2);
        let inv = Invite { endpoint_id: SecretKey::generate().public(), token: new_token(), relay_url: None };
        let sealed = seal_for_code(&inv, &e1).unwrap();
        assert!(sealed.len() <= 245);
        let back = open_from_code(&sealed, &e2).unwrap();
        assert_eq!(back.endpoint_id, inv.endpoint_id);
        let other = derive_code_keys(&new_short_code()).unwrap().1;
        assert!(open_from_code(&sealed, &other).is_err());
    }
}
