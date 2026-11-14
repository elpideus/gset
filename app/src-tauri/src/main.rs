fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // for working on the window in a browser: gset-app --dump-fixture DIR
    if let [flag, dir] = args.as_slice()
        && flag == "--dump-fixture"
    {
        match gset_app::views::dump_fixture(std::path::Path::new(dir)) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("gset-app: {e}");
                std::process::exit(1);
            }
        }
    }
    // gset-app --refresh-profiles: download the community's profiles now, without the window
    if args.first().map(String::as_str) == Some("--refresh-profiles") {
        match tauri::async_runtime::block_on(gset_app::profiles::refresh()) {
            Ok(done) => println!(
                "{}",
                serde_json::to_string_pretty(&done).unwrap_or_default()
            ),
            Err(e) => {
                eprintln!("gset-app: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    gset_app::start();
}
