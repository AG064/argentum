//! Minimal preview host for the generated Argentum shell.
//!
//! The binary renders the compiled `MainWindow` with its default values. It
//! carries no application state and no mock content, so it exists only to run
//! the shell on its own while `SLINT_LIVE_PREVIEW` is set.

fn main() -> Result<(), slint::PlatformError> {
    use slint::ComponentHandle;

    let window = argentum_ui::MainWindow::new()?;
    window.run()
}
