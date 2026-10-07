use resume_core::{Store, backup::RestoreFault};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut store = Store::open(&args[2]).expect("probe store");
    match args[1].as_str() {
        "commitExit" => {
            let profile = store.profile().unwrap();
            let mut draft = profile.content;
            draft.name = "退出前已提交".into();
            store.save_profile(draft, profile.revision).unwrap();
            std::process::exit(74);
        }
        "restore" => {
            let package = std::path::Path::new(&args[3]);
            let preview = store.inspect_backup(package).unwrap();
            store
                .restore(package, &preview.package_sha256, RestoreFault::None)
                .unwrap();
        }
        _ => panic!("unknown probe mode"),
    }
}
