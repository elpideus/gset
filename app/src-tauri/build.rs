fn main() {
    // The launcher, as scripts/sidecar.mjs put it for bundling: the window also carries it inside
    // itself (src/launcher.rs), untouched by the AppImage's packager.
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let launcher = std::path::Path::new(&dir).join(format!("binaries/gset-{target}"));
    println!("cargo:rerun-if-changed={}", launcher.display());
    println!("cargo:rustc-env=GSET_LAUNCHER_PATH={}", launcher.display());
    tauri_build::build();
}
