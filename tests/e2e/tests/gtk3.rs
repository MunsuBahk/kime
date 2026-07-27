//! GTK3 immodule end-to-end tests (staged local `libim-kime.so` on Xvfb).
//!
//! Run: `cargo test -p kime-e2e --test gtk3 -- --ignored --test-threads=1`

use std::path::Path;
use std::time::Duration;

use kime_e2e::kime;
use kime_e2e::paths::{self, ScratchDir};
use kime_e2e::probes::{self, GtkProbeOpts, GuiProbe, ImEvent, PROBE_TITLE};
use kime_e2e::proc;
use kime_e2e::stage::{self, Staged};
use kime_e2e::x11::XvfbSession;

const WAIT: Duration = Duration::from_secs(10);

/// Stage the local GTK3 immodule, write a Hangul config, and spawn the probe
/// on `x` with `extra_env` appended. The caller must still
/// `focus_window(PROBE_TITLE)` before typing.
fn spawn_probe(
    x: &XvfbSession,
    scratch: &ScratchDir,
    textview: bool,
    extra_env: &[(String, String)],
) -> (GuiProbe, Staged) {
    let staged = stage::stage_gtk3(scratch).expect("stage gtk3 immodule");
    let xdg = kime::write_config(scratch, kime::HANGUL_CONFIG).expect("write kime config");
    let mut env = staged.env.clone();
    env.push(("XDG_CONFIG_HOME".into(), xdg.display().to_string()));
    env.extend_from_slice(extra_env);
    let opts = GtkProbeOpts {
        gtk_major: 3,
        textview,
    };
    let probe = probes::spawn_gtk_probe_x11(x, &env, &opts, scratch).expect("start gtk3 probe");
    (probe, staged)
}

/// Number of IM events logged so far (marker for [`wait_new_preedit`]).
fn im_len(log: &Path) -> usize {
    probes::read_im_log(log).len()
}

/// Wait until at least one new IM event arrived after `since` and the latest
/// event is `Preedit(expected)`. Only usable with the Entry probe — GTK3
/// TextView has no `preedit-changed` signal.
fn wait_new_preedit(log: &Path, expected: &str, since: usize) {
    proc::wait_until(
        &format!(
            "preedit {expected:?} in {} (events: {:?})",
            log.display(),
            probes::read_im_log(log)
        ),
        WAIT,
        || {
            let events = probes::read_im_log(log);
            events.len() > since
                && matches!(events.last(), Some(ImEvent::Preedit(s)) if s == expected)
        },
    )
    .expect("preedit did not appear");
}

/// Assert the non-empty preedit stream contains `expected` in order.
fn assert_preedit_sequence(log: &Path, expected: &[&str]) {
    let seq: Vec<String> = probes::read_im_log(log)
        .into_iter()
        .filter_map(|e| match e {
            ImEvent::Preedit(s) if !s.is_empty() => Some(s),
            _ => None,
        })
        .collect();
    let mut iter = seq.iter();
    for want in expected {
        assert!(
            iter.any(|s| s == want),
            "preedit stream {seq:?} does not contain {expected:?} in order"
        );
    }
}

/// Live child pids of `pid` (all threads' `/proc/<pid>/task/*/children`).
/// Zombie children stay listed until reaped, which is exactly what the #617
/// test needs to observe.
fn child_pids(pid: i32) -> Vec<i32> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/task")) {
        for entry in entries.flatten() {
            if let Ok(s) = std::fs::read_to_string(entry.path().join("children")) {
                out.extend(s.split_whitespace().filter_map(|p| p.parse::<i32>().ok()));
            }
        }
    }
    out
}

/// `/proc/<pid>/comm` (kernel-truncated to 15 chars).
fn comm(pid: i32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Process state char from `/proc/<pid>/stat` (`Z` = zombie/defunct).
fn stat_state(pid: i32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 3 comes after the parenthesized comm, which may itself contain
    // spaces/parens — split at the LAST ')'.
    stat.rsplit(')')
        .next()?
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// `kime-candidate-window` children of `pid` (comm is truncated to 15 chars,
/// so match on the prefix).
fn candidate_children(pid: i32) -> Vec<i32> {
    child_pids(pid)
        .into_iter()
        .filter(|&c| comm(c).starts_with("kime-candidate"))
        .collect()
}

/// G3-SMOKE: Xvfb → staged local `libim-kime.so` (private immodule cache via
/// `gtk-query-immodules-3.0` + `GTK_IM_MODULE_FILE`) → GTK3 Entry probe.
///
/// Types `gksrmf` + Enter (dubeolsik) and asserts the committed text `한글`
/// with the full preedit stream `ㅎ하한ㄱ그글`, plus a `/proc/<pid>/maps`
/// check that the LOCAL staged module loaded.
#[test]
#[ignore = "e2e: spawns Xvfb and a GTK3 app; run with --ignored"]
fn g3_smoke() {
    let scratch = ScratchDir::new("g3_smoke");
    let x = XvfbSession::new(&scratch).expect("start Xvfb");
    let (probe, staged) = spawn_probe(&x, &scratch, false, &[]);
    x.focus_window(PROBE_TITLE).expect("focus probe window");

    x.type_text("gksrmf").expect("type gksrmf");
    x.key("Return").expect("press Return");

    probe.buffer.wait_for("한글", WAIT).expect("committed 한글");
    stage::maps_check_staged(probe.pid(), &staged).expect("local gtk3 immodule loaded");
    assert_preedit_sequence(&probe.preedit_log, &["ㅎ", "하", "한", "ㄱ", "그", "글"]);
}

/// G3-01 (PR #775 GTK3 guard): the GTK3 deferred re-injection path is
/// unchanged — the same input as G4-01 yields the identical buffer.
///
/// PR #775 fixed the GTK4 build synchronously but intentionally kept GTK3's
/// `gdk_event_put` deferral (workaround for #536/#565/#570 app quirks); this
/// pins that behavior: `dks` + one Return → `안\n`, twice → `안\n안\n`.
/// GTK3 TextView has no `preedit-changed` signal, so this asserts on the
/// buffer only, with a short settle pause after typing.
#[test]
#[ignore = "e2e: spawns Xvfb and a GTK3 app; run with --ignored"]
fn g3_01_775_deferral_path_unchanged() {
    let scratch = ScratchDir::new("g3_01_775");
    let x = XvfbSession::new(&scratch).expect("start Xvfb");
    let (probe, staged) = spawn_probe(&x, &scratch, true, &[]);
    x.focus_window(PROBE_TITLE).expect("focus probe window");

    x.type_text("dks").expect("type dks");
    // No preedit-changed on GTK3 TextView; xdotool paces keys at 100ms and the
    // engine runs in-process, so a short settle is enough for preedit 안.
    proc::sleep_ms(500);
    stage::maps_check_staged(probe.pid(), &staged).expect("local gtk3 immodule loaded");
    x.key("Return").expect("press Return");
    probe
        .buffer
        .wait_for("안\n", WAIT)
        .expect("single Enter must commit 안 and insert the newline (GTK3 deferral)");

    x.type_text("dks").expect("type dks");
    proc::sleep_ms(500);
    x.key("Return").expect("press Return");
    probe
        .buffer
        .wait_for("안\n안\n", WAIT)
        .expect("second dks+Enter round must yield 안\\n안\\n");
}

/// G3-02 (#617 zombie half, PR #769 fix 2): dismissed hanja candidate popups
/// leave no `<defunct>` `kime-candidate-window` children behind.
///
/// The engine runs in-process in the probe, so the candidate window spawns as
/// a child of the probe pid. Loop 3×: `gks` → preedit 한 → Control_R opens
/// the popup (default `category_hotkeys` config) → BackSpace at the probe
/// cancels hanja mode via `HanjaMode::press_key` → `reset`, running
/// `Client::close`'s kill+reap path (`src/engine/candidate/src/client.rs`).
/// Pre-fix, `close` killed the child but never reaped it, accumulating one
/// zombie per dismissed popup (#617).
///
/// Esc is deliberately NOT used to dismiss: global hotkeys are merged into
/// `mode_hotkeys`, so in Hanja mode Esc matches `!Switch Latin` and
/// `Engine::set_input_category` clears `mode` WITHOUT closing the hanja
/// client — the popup process leaks (separate kime bug found while writing
/// this test; see the harness report). Any non-hotkey key reaches
/// `HanjaMode::press_key` and the close path under test.
#[test]
#[ignore = "e2e: spawns Xvfb, a GTK3 app, and kime-candidate-window; run with --ignored"]
fn g3_02_617_no_zombie_candidates() {
    let scratch = ScratchDir::new("g3_02_617");
    let x = XvfbSession::new(&scratch).expect("start Xvfb");
    // The candidate window is eframe/egui (glow) → software GL; the engine
    // spawns `kime-candidate-window` via PATH, so put the local target dir
    // first (system kime is installed on dev machines).
    let extra_env = vec![
        ("LIBGL_ALWAYS_SOFTWARE".into(), "1".into()),
        (
            "PATH".into(),
            format!(
                "{}:{}",
                paths::target_dir().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        ),
    ];
    let (probe, staged) = spawn_probe(&x, &scratch, false, &extra_env);
    x.focus_window(PROBE_TITLE).expect("focus probe window");

    for round in 1..=3 {
        // The popup may have taken focus in a previous round — refocus.
        x.focus_window(PROBE_TITLE).expect("focus probe window");
        let since = im_len(&probe.preedit_log);
        x.type_text("gks").expect("type gks");
        wait_new_preedit(&probe.preedit_log, "한", since);
        if round == 1 {
            stage::maps_check_staged(probe.pid(), &staged).expect("local gtk3 immodule loaded");
        }

        x.key("Control_R").expect("press Control_R");
        let mut child = 0;
        proc::wait_until(
            &format!("kime-candidate-window child of probe (round {round})"),
            Duration::from_secs(15),
            || match candidate_children(probe.pid()).first() {
                Some(&c) => {
                    child = c;
                    true
                }
                None => false,
            },
        )
        .expect("hanja candidate window did not spawn");

        // BackSpace at the probe: HanjaMode::press_key → reset → Client::close
        // (kill + reap). A zombie child stays listed in /proc children until
        // reaped, so this wait times out on the pre-#769 kill-without-wait.
        x.key("BackSpace").expect("press BackSpace");
        proc::wait_until(
            &format!("candidate child {child} to be closed AND reaped (round {round})"),
            WAIT,
            || !child_pids(probe.pid()).contains(&child),
        )
        .expect("candidate window child never reaped — #617 zombie regression");
    }

    let leftovers = candidate_children(probe.pid());
    assert!(
        leftovers.is_empty(),
        "candidate window children survived the loop: {leftovers:?}"
    );
    let zombies: Vec<i32> = child_pids(probe.pid())
        .into_iter()
        .filter(|&c| stat_state(c) == Some('Z'))
        .collect();
    assert!(
        zombies.is_empty(),
        "#617 regression: defunct children of the probe remain: {zombies:?}"
    );
}
