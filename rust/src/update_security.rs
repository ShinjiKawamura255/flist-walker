use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

pub const UPDATE_PUBLIC_KEY_ENV: &str = "FLISTWALKER_UPDATE_PUBLIC_KEY_HEX";
pub const UPDATE_SIGNING_KEY_ENV: &str = "FLISTWALKER_UPDATE_SIGNING_KEY_HEX";
pub const CHECKSUM_SIGNATURE_NAME: &str = "SHA256SUMS.sig";

pub fn embedded_public_key_hex() -> Option<&'static str> {
    option_env!("FLISTWALKER_UPDATE_PUBLIC_KEY_HEX").filter(|value| !value.trim().is_empty())
}

pub fn has_embedded_public_key() -> bool {
    embedded_public_key_hex().is_some()
}

pub fn verify_embedded_signature(message: &[u8], signature_bytes: &[u8]) -> Result<()> {
    let public_key = embedded_public_key_hex()
        .context("update signature verification key is not embedded in this build")?;
    verify_signature(message, signature_bytes, public_key)
}

pub fn verify_signature(
    message: &[u8],
    signature_bytes: &[u8],
    public_key_hex: &str,
) -> Result<()> {
    let verifying_key = verifying_key_from_hex(public_key_hex)?;
    let signature = signature_from_bytes(signature_bytes)?;
    verifying_key
        .verify(message, &signature)
        .context("update signature verification failed")
}

pub fn sign_with_env_key(message: &[u8]) -> Result<Vec<u8>> {
    let key_hex = std::env::var(UPDATE_SIGNING_KEY_ENV)
        .context("FLISTWALKER_UPDATE_SIGNING_KEY_HEX is not set")?;
    sign_message(message, &key_hex)
}

pub fn sign_message(message: &[u8], signing_key_hex: &str) -> Result<Vec<u8>> {
    let signing_key = signing_key_from_hex(signing_key_hex)?;
    Ok(signing_key.sign(message).to_bytes().to_vec())
}

fn verifying_key_from_hex(hex: &str) -> Result<VerifyingKey> {
    let bytes = decode_hex_fixed::<32>(hex)
        .with_context(|| format!("invalid {UPDATE_PUBLIC_KEY_ENV} value"))?;
    VerifyingKey::from_bytes(&bytes).context("failed to parse update public key")
}

fn signing_key_from_hex(hex: &str) -> Result<SigningKey> {
    let bytes = decode_hex_fixed::<32>(hex)
        .with_context(|| format!("invalid {UPDATE_SIGNING_KEY_ENV} value"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn signature_from_bytes(bytes: &[u8]) -> Result<Signature> {
    let raw: [u8; 64] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid signature length: expected 64 bytes"))?;
    Ok(Signature::from_bytes(&raw))
}

fn decode_hex_fixed<const N: usize>(hex: &str) -> Result<[u8; N]> {
    let bytes = decode_hex(hex)?;
    let len = bytes.len();
    let fixed: [u8; N] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected {N} decoded bytes, got {}", len))?;
    Ok(fixed)
}

fn decode_hex(hex: &str) -> Result<Vec<u8>> {
    let normalized = hex.trim();
    if !normalized.len().is_multiple_of(2) {
        bail!("hex string must have even length");
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let hi = decode_nibble(bytes[idx])?;
        let lo = decode_nibble(bytes[idx + 1])?;
        out.push((hi << 4) | lo);
        idx += 2;
    }
    Ok(out)
}

fn decode_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex character: {}", byte as char),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SIGNING_KEY_HEX: &str =
        "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const TEST_PUBLIC_KEY_HEX: &str =
        "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664";

    #[test]
    fn sign_and_verify_round_trip() {
        let message = b"sha manifest";
        let signature = sign_message(message, TEST_SIGNING_KEY_HEX).expect("sign");
        verify_signature(message, &signature, TEST_PUBLIC_KEY_HEX).expect("verify");
    }

    #[test]
    fn verify_signature_rejects_tampered_message() {
        let signature = sign_message(b"sha manifest", TEST_SIGNING_KEY_HEX).expect("sign");
        let err = verify_signature(b"sha manifest tampered", &signature, TEST_PUBLIC_KEY_HEX)
            .expect_err("tampered message should fail");
        assert!(err.to_string().contains("verification failed"));
    }
}
