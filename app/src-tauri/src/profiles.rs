//! The community's game profiles (github.com/elpideus/gset-profiles), downloaded when the window
//! opens. The bundle is signed with GSet's update key, and a bundle that doesn't verify against the
//! public key built into GSet is thrown away. Profiles only describe games: installing them never
//! changes a value of yours (see gset_core::profile::install_bundle).

use base64::Engine;
use gset_core::Paths;
use gset_core::profile::{self, Installed};
use std::time::Duration;

const URL: &str =
    "https://github.com/elpideus/gset-profiles/releases/download/bundle/profiles.json";

/// The public half of GSet's update key (tauri.conf.json's updater pubkey, via build.rs).
const PUBKEY: &str = env!("GSET_PUBKEY");

/// Checks a Tauri-style signature: both the key and the signature are base64 of minisign's text.
pub fn verify(data: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let text = |s: &str| -> Result<String, String> {
        let bytes = b64.decode(s.trim()).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string())
    };
    let key = minisign_verify::PublicKey::decode(&text(pubkey_b64)?)
        .map_err(|e| format!("the key: {e}"))?;
    let sig = minisign_verify::Signature::decode(&text(signature_b64)?)
        .map_err(|e| format!("the signature: {e}"))?;
    key.verify(data, &sig, false)
        .map_err(|_| "the profile bundle's signature doesn't match GSet's key".to_string())
}

async fn download() -> Result<(Vec<u8>, String), String> {
    // the same TLS setup as the updater (rustls with ring); installing it twice is harmless
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent(concat!("GSet/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let get = |url: String| {
        let client = client.clone();
        async move {
            let r = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("couldn't reach GitHub: {e}"))?;
            r.error_for_status()
                .map_err(|e| format!("GitHub answered {e}"))?
                .bytes()
                .await
                .map_err(|e| e.to_string())
        }
    };
    // GSET_PROFILES_URL: another bundle (a local one, to test); its signature must still verify
    let url = std::env::var("GSET_PROFILES_URL").unwrap_or_else(|_| URL.to_string());
    let data = get(url.clone()).await?;
    let sig = get(format!("{url}.sig")).await?;
    Ok((data.to_vec(), String::from_utf8_lossy(&sig).into_owned()))
}

/// Downloads the bundle, checks it, and installs what's new.
pub async fn refresh() -> Result<Installed, String> {
    let (data, sig) = download().await?;
    verify(&data, &sig, PUBKEY)?;
    let bundle = profile::read_bundle(std::str::from_utf8(&data).map_err(|e| e.to_string())?)?;
    let paths = Paths::new();
    let app = gset_core::AppConfig::load(&paths)?;
    let out = profile::install_bundle(&paths, &bundle, app.auto_profile_updates);
    let _ = std::fs::write(
        paths.data.join("profiles-checked"),
        gset_core::clock::now_text(),
    );
    Ok(out)
}

#[tauri::command]
pub async fn refresh_profiles() -> Result<Installed, String> {
    refresh().await
}

#[cfg(test)]
mod tests {
    // made with a throwaway key: `tauri signer generate --ci`, then `tauri signer sign data.json`
    const KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRFOUE5NzkyRjVGQTgxMEEKUldRS2dmcjFrcGVhVGlySUwvaWkvdUJ0MHI3MW5NbkhvL2VvUUcrWnRkcDZSRmE4Z203Nm80V2MK";
    const SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRS2dmcjFrcGVhVHFpS1V3SHp1VGJLcjlRbkVxYUt6UzFzUDZ3MHJrdHRBSHhYY3FGRGNqOHVtZEZ6RWdqR1FBbGdQVThvbEQxZ2prTlY4aHhHd3FveU9NOXZKTGlvVkFzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NjcxMzMxCWZpbGU6ZGF0YS5qc29uClNuK29zekNpN1orazVrL3lvcVA5NXROdkZRdCt1dzZFb1N0VWxNSXo1bDlrYW5EWnNMeHorZ1gzc2I3ZTNWYVRTYzN3M0ZZYUxOUXNZMUs2c0Q1NENnPT0K";
    const DATA: &[u8] = b"{\"schema\":1,\"profiles\":{}}\n";

    #[test]
    fn signatures() {
        assert!(super::verify(DATA, SIG, KEY).is_ok());
        assert!(
            super::verify(b"{\"schema\":1,\"profiles\":{\"x\":\"\"}}\n", SIG, KEY).is_err(),
            "a changed bundle fails"
        );
        assert!(
            super::verify(DATA, SIG, super::PUBKEY).is_err(),
            "another key fails"
        );
    }
}
