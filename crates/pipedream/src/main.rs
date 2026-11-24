//! Native player: `pipedream [path/to/Sushi.ati]`.
// No console window on Windows; errors go to a message box.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(e) = native::run() {
        eprintln!("{e}");
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Pipe Dream")
            .set_description(e.to_string())
            .show();
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
