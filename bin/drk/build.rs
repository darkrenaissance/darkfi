use pkg_config::probe_library;

fn main() {
    if probe_library("libout123").is_ok() && probe_library("libmpg123").is_ok() {
        println!("cargo:rustc-cfg=feature=\"play\"");
    }
}
