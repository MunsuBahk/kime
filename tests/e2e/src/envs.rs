//! Environment hygiene: every spawned process starts from `env_clear()` plus a
//! small allowlist, so the harness can never leak the user's live session
//! (`WAYLAND_DISPLAY`, `DISPLAY`, `GTK_IM_MODULE=kime`, ...) into a test.

use std::ffi::OsStr;
use std::process::Command;

/// Variables inherited from the harness environment. Everything else —
/// including `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_SESSION_*`, `*_IM_MODULE`,
/// `XMODIFIERS` — is dropped and must be set explicitly per test.
pub const ALLOWLIST: &[&str] = &["PATH", "HOME", "XDG_RUNTIME_DIR", "LANG"];

/// A `Command` with a cleared environment plus [`ALLOWLIST`].
/// Falls back to `LANG=C.UTF-8` when the host has no `LANG` (XIM needs a
/// UTF-8 locale).
pub fn clean_cmd(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear();
    for key in ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    if std::env::var_os("LANG").is_none() {
        cmd.env("LANG", "C.UTF-8");
    }
    cmd
}

/// `XDG_RUNTIME_DIR`, which wayland sockets require. Errors (instead of
/// silently producing confusing connect failures) when unset.
pub fn xdg_runtime_dir() -> crate::Result<String> {
    std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR is not set; wayland tests need it".to_string())
}
