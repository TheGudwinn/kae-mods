//! Checks that the signature of a locally built installer matches the public key in tauri.conf.json.
//! Needs a prior `pnpm build:local`, so it is ignored by default:
//! `cargo test --test release_signature -- --ignored`
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};

fn decode(b64: &str) -> String {
    String::from_utf8(STANDARD.decode(b64.trim()).unwrap()).unwrap()
}

#[test]
#[ignore = "needs a locally built signed installer"]
fn installer_signature_matches_the_public_key_in_the_config() {
    let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("tauri.conf.json").unwrap()).unwrap();
    let pubkey = decode(conf["plugins"]["updater"]["pubkey"].as_str().unwrap());
    let version = conf["version"].as_str().unwrap();
    let dir = std::path::Path::new("target/release/bundle/nsis");
    let exe = dir.join(format!("Kae Mods_{version}_x64-setup.exe"));
    let sig = dir.join(format!("Kae Mods_{version}_x64-setup.exe.sig"));

    let pk = PublicKey::decode(&pubkey).expect("public key in the config is valid");
    let signature = Signature::decode(&decode(&std::fs::read_to_string(sig).unwrap())).expect("signature file is valid");
    let mut bytes = std::fs::read(&exe).unwrap();
    pk.verify(&bytes, &signature, false).expect("installer is signed by the key whose public half is in the config");

    // A single changed byte must be rejected.
    let mid = bytes.len() / 2;
    bytes[mid] ^= 1;
    assert!(pk.verify(&bytes, &signature, false).is_err());
}
