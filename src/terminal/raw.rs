pub struct RawGuard;

impl RawGuard {
    pub fn enable() -> Self {
        let _ = crossterm::terminal::enable_raw_mode();
        Self
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        print!("\x1b[0m");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}
