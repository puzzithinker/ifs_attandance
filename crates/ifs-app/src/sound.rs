//! Soft audio cues for scan success / failure (toggleable).
//!
//! Uses ASCII BEL; fails soft if the environment ignores it (headless GUI).

/// Play a short success cue when `enabled`.
pub fn play_success(enabled: bool) {
    if enabled {
        beep();
    }
}

/// Play a failure/warning cue when `enabled` (double BEL when possible).
pub fn play_failure(enabled: bool) {
    if enabled {
        beep();
        // tiny delay then second beep for fail distinction
        std::thread::sleep(std::time::Duration::from_millis(80));
        beep();
    }
}

fn beep() {
    use std::io::Write;
    let _ = std::io::stderr().write_all(b"\x07");
    let _ = std::io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_does_not_panic() {
        play_success(false);
        play_failure(false);
    }

    #[test]
    fn enabled_does_not_panic() {
        // May or may not produce audible sound; must not panic.
        play_success(true);
    }
}
