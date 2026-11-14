fn main() {
    // The launcher, as scripts/sidecar.mjs put it for bundling: the window also carries it inside
    // itself (src/launcher.rs), untouched by the AppImage's packager.
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let launcher = std::path::Path::new(&dir).join(format!("binaries/gset-{target}"));
    println!("cargo:rerun-if-changed={}", launcher.display());
    println!("cargo:rustc-env=GSET_LAUNCHER_PATH={}", launcher.display());
    // the public half of the update key, which also signs the profile bundle (src/profiles.rs)
    let conf = std::fs::read_to_string(std::path::Path::new(&dir).join("tauri.conf.json"))
        .expect("tauri.conf.json");
    let key = conf
        .split("\"pubkey\": \"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .expect("tauri.conf.json has the updater's pubkey");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rustc-env=GSET_PUBKEY={key}");
    tauri_build::build();
}
