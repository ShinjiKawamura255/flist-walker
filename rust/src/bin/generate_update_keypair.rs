use ed25519_dalek::SigningKey;
use rand_core::OsRng;

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn main() {
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    println!(
        "FLISTWALKER_UPDATE_SIGNING_KEY_HEX={}",
        encode_hex(&signing_key.to_bytes())
    );
    println!(
        "FLISTWALKER_UPDATE_PUBLIC_KEY_HEX={}",
        encode_hex(&verifying_key.to_bytes())
    );
}
