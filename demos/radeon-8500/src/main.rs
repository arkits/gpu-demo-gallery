#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = radeon_8500::run_native() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
