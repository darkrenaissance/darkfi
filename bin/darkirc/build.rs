fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "android" {
        println!("cargo:rustc-link-search={}/sqlcipher", env!("CARGO_MANIFEST_DIR"));
    }
}
