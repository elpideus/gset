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
    gset_app::start();
}
