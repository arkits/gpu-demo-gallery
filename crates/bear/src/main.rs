//! Native player: `bear [path/to/Sushi.ati]`.
// No console window on Windows; errors go to a message box.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(e) = native::run() {
        eprintln!("{e}");
        // In a terminal the line above is enough; from a desktop, show a dialog.
        if !std::io::IsTerminal::is_terminal(&std::io::stderr()) && std::env::args().len() == 1 {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Bear")
                .set_description(e.to_string())
                .show();
        }
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
