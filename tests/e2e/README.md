# kime-e2e — end-to-end GUI regression tests

Headless end-to-end tests for every kime frontend: **wayland**
(`kime-wayland` on a headless sway), **gtk3 / gtk4** (staged immodules on
Xvfb), **qt5 / qt6** (staged input-context plugins on Xvfb, plus a
Qt-on-Wayland case), and **xim** (`kime-xim` on Xvfb). Each test spawns its
own compositor/X server, injects real key events, and asserts what an
application actually receives — committed text, preedit streams, protocol
traffic, child processes.

All tests are `#[ignore]`, so a plain workspace `cargo test` stays green and
CI is unaffected. Nothing here needs root, uinput, or a running desktop; the
harness never touches the inherited `WAYLAND_DISPLAY`/`DISPLAY`.

## Prerequisites

Arch package names (the suite was developed against these):

| What | Arch package |
|---|---|
| C/C++ toolchain + pkg-config | `base-devel`, `pkgconf` |
| Headless Wayland compositor | `sway` |
| Headless X server | `xorg-server-xvfb` |
| X11 key/window driving | `xdotool` |
| XTEST + Xlib headers (C clients) | `libxtst`, `libx11` |
| wayland-scanner + client libs | `wayland` |
| xkbcommon (injector keymap) | `libxkbcommon` |
| GTK probes | `python-gobject`, `gtk3`, `gtk4` |
| Qt probes | `qt5-base`, `qt6-base`, `qt6-wayland` (Q-02 only) |
| Software GL for the candidate popup | `mesa` (llvmpipe) |

**nix devshell:** `nix develop` provides all of the above
(`kimeE2eBuildInputs` / `kimeE2eNativeBuildInputs` in `nix/deps.nix`) except
Qt5 — nixpkgs refuses Qt5 and Qt6 in one environment, so that build has no
qt5 plugin and `q5_smoke` skips. This is the environment CI runs the suite
in; see the `e2e` job in `.github/workflows/ci.yaml`.

## Building the artifacts under test

The tests exercise the **local** build products, never the system install:

```sh
meson setup build --buildtype=debug -Dcargo_profile=debug
ninja -C build
```

This produces the cargo binaries + `libkime_engine.so` in `target/debug/`
and the C/C++ immodules in `build/src/frontends/{gtk3,gtk4,qt5,qt6}/`. Every
immodule test verifies via `/proc/<pid>/maps` that the locally staged module
loaded (system-installed copies would otherwise shadow it silently), and
every kime process runs with `LD_LIBRARY_PATH` pointing at `target/debug`
(a system `libkime_engine.so` may be stale).

## Running

```sh
# everything (serial — required, tests own displays and sockets):
cargo test -p kime-e2e -- --ignored --test-threads=1
# or, rebuild first:
tests/e2e/run.sh

# one frontend:
cargo test -p kime-e2e --test wayland -- --ignored --test-threads=1

# one test:
cargo test -p kime-e2e --test wayland -- --ignored w01_714_no_spurious_commit
```

Env overrides:

| Variable | Default | Meaning |
|---|---|---|
| `KIME_E2E_BUILD_DIR` | `<repo>/build` | meson build dir (immodules) |
| `KIME_E2E_TARGET_DIR` | `<repo>/target/debug` | cargo output dir (binaries + engine .so) |
| `KIME_E2E_KEEP_LOGS=1` | unset | keep per-test scratch dirs even on success |
| `KIME_E2E_PASS_ENV` | unset | comma-separated names to forward through the `env_clear` allowlist |

`KIME_E2E_PASS_ENV` exists for environments that keep their GTK/Qt/GL
runtime off the paths those libraries search by default: `shell.nix` sets it
to hand probes `GI_TYPELIB_PATH`, `LIBGL_DRIVERS_PATH`, `FONTCONFIG_FILE`
and friends. Never list session state (`DISPLAY`, `WAYLAND_DISPLAY`,
`*_IM_MODULE`) — the allowlist drops those on purpose.

Do not run two instances of the suite from the same checkout at once: they
share `target/e2e/` (scratch dirs) and `target/e2e-clients/` (compiled probe
binaries), and concurrent runs can interfere.

On failure a test keeps its scratch dir (`target/e2e/<test>-<pid>-<nonce>/`)
and prints the path; it contains compositor logs, `WAYLAND_DEBUG` traces,
probe buffer/preedit dumps, and the per-test kime config.

## Architecture

```
tests/e2e/
├── src/            the harness library (kime-e2e)
│   ├── paths.rs    artifact discovery + ScratchDir (kept on failure)
│   ├── proc.rs     Proc RAII guard (kills only its own pid), wait_for_line,
│   │               wait_until — no pkill, no bare sleeps for readiness
│   ├── envs.rs     clean_cmd(): env_clear + allowlist (PATH, HOME,
│   │               XDG_RUNTIME_DIR, LANG) + per-test vars; the live session
│   │               env can never leak in
│   ├── cc.rs       on-demand compiler for the C/C++ clients (memoized into
│   │               target/e2e-clients/, wayland-scanner glue from
│   │               clients/protocols/)
│   ├── x11.rs      XvfbSession (free display probed, retries on collision),
│   │               xdotool wrappers, raw-keycode XTEST helper
│   ├── sway.rs     SwaySession: headless sway -V, socket parsed from the log
│   ├── inject.rs   VkbdInjector: zwp_virtual_keyboard_v1 injector driven
│   │               over a fifo; MUST start before KimeWayland (see below)
│   ├── kime.rs     KimeWayland / KimeXim spawners + config writer
│   ├── stage.rs    immodule staging (GTK_IM_MODULE_FILE cache, GTK_PATH
│   │               layout, QT_PLUGIN_PATH) + /proc/maps local-module check
│   ├── probes.rs   GUI probe launchers + BufferWatcher (polls the probes'
│   │               atomically-replaced dump files) + IM-event log parser
│   └── wldebug.rs  WAYLAND_DEBUG trace slicing/counting
├── clients/        vendored probe sources, compiled on demand
│   ├── gtk_probe.py    GTK3/GTK4 Entry/TextView probe (buffer + cursor +
│   │                   preedit dumps)
│   ├── qt_probe.cpp    Qt5/Qt6 QLineEdit probe + QInputMethodEvent log
│   ├── xim_client.c    minimal PreeditNothing XIM client (see bug note below)
│   ├── vkbd_inject.c   virtual-keyboard injector (tap/press/release)
│   ├── wlkbd_probe.c   plain wl_keyboard client without text-input (#744)
│   ├── xtest_key.c     raw-keycode XTEST injector (#721/#603)
│   └── protocols/      vendored protocol XMLs for wayland-scanner
└── tests/          one file per frontend: wayland.rs, gtk4.rs, gtk3.rs,
                    qt.rs, xim.rs (+ clients.rs harness self-check)
```

A typical Wayland test: `SwaySession` → `VkbdInjector` → `KimeWayland`
(per-test `XDG_CONFIG_HOME`, `WAYLAND_DEBUG=1` trace) → GTK4 text-input-v3
probe → inject evdev codes → assert buffer/preedit/protocol trace. A typical
X11 test: `XvfbSession` → staged immodule or `kime-xim` → probe → `xdotool`.
Teardown is RAII by exact pid in reverse order; tests must be run with
`--test-threads=1`.

## Adding a test

1. Pick the frontend file in `tests/` (or add a new one for a new frontend
   and register nothing — cargo picks `tests/*.rs` up automatically).
2. Name it after the bug: `w05_1234_short_description`, with a doc comment
   citing the issue and fix PR and describing the pre-fix failure shape.
3. Mark it `#[ignore = "e2e: spawns …; run with --ignored"]`.
4. Reuse the fixtures: sessions and probes are constructed per test —
   never share state across tests, never hardcode display/socket names.
5. Synchronize on observable events (`wait_for_line`, `BufferWatcher::
   wait_for`, `wait_until`), not sleeps. Timed *holds* (key repeat) are the
   only inherent durations.
6. If it needs a new C client, drop the source in `clients/` and add a
   compile entry in `src/cc.rs` (+ the self-check in `tests/clients.rs`).

## Traceability: bug ↔ test ↔ fix

| Bug | Test | Fix ref |
|---|---|---|
| #714/#654/#668/#528 spurious empty IM commit clobbers selection | `wayland::w01_714_no_spurious_commit` | PR #772 fix A (+#718) |
| #666 key repeat stalls at preedit/text boundary | `wayland::w02_666_repeat_crosses_boundary` | PR #772 fix B |
| #744 keys dead without an enabled text input | `wayland::w03_744_bypass_without_text_input` | PR #745 |
| #603 volume keys toggled IME / opened hanja (wayland `+8` path) | `wayland::w04_603_volume_key_bypassed` | PR #769 fix 1 |
| #603 (xim `xev.detail` path) | `xim::x02_603_volume_key_bypassed` | PR #769 fix 1 |
| #606/#613 GTK4 Enter/Tab/arrows swallowed during preedit | `gtk4::g4_01_606_enter_commits_and_newlines`, `gtk4::g4_02_606_tab_and_arrow_reach_widget` | PR #775 fix 1 |
| PR #775 GTK3 guard: deferral path unchanged (#536/#565/#570) | `gtk3::g3_01_775_deferral_path_unchanged` | PR #775 fix 1 |
| #617 zombie kime-candidate-window children | `gtk3::g3_02_617_no_zombie_candidates` | PR #769 fix 2 |
| #757 Qt candidate popup killed on focus loss | `qt::q01_757_candidate_survives_focus_loss` (currently SKIPs: residual bug, see below) | PR #771 (incomplete) |
| #760 AltR toggle dead with self-modifier bit | `qt::q02_760_altr_self_modifier_e2e` | PR #760 |
| #721/#731 kime-xim dies on unmapped keycode | `xim::x01_721_survives_unmapped_keycode` | PR #722 |
| #736/#756 Qt plugin IID / plugin loads no IM | `qt::q6_smoke`, `qt::q5_smoke` (currently SKIPs: qt5 recipe bug, see below) | #736 + #756 (qt6 only) |
| harness/pipeline guards | `wayland::w_smoke`, `gtk3::g3_smoke`, `gtk4::g4_smoke`, `xim::x_smoke`, `clients::build_all_clients` | — |

Not implemented (deliberately): #579 UAF (needs a purpose-built ASAN
harness), #754 (fully unit-covered in `src/engine/core/tests/`), #666 v1
path via Weston headless, #769 fix 3 (log-only, not GUI-observable).

## Known product bugs the suite works around

Found during harness development and left **unfixed in product code** (the
suite documents rather than hides them):

1. **kime-wayland crashes on an empty keymap from a keyboardless seat.** On
   a seat with no keyboard, sway sends `keymap(format=0, size=0)`;
   kime-wayland forwards it verbatim to its virtual keyboard, wlroots
   rejects it, and kime aborts (panic at `src/frontends/wayland/src/main.rs`
   roundtrip). **Workaround baked into the harness:** `VkbdInjector` must be
   constructed *before* `KimeWayland` so the seat already has a real keymap.
   Real fix: skip forwarding `format=0/size=0` keymaps.
2. **kime-xim + GTK3 as an XIM client enters an infinite preedit echo
   loop** (PreeditDraw/Done/Start cycling at ~6000 events/s; nothing ever
   commits). The XIM tests therefore use the minimal C `PreeditNothing`
   client (`clients/xim_client.c`) — do not "upgrade" them to a GTK probe.
3. **`q5_smoke` SKIPs:** `src/frontends/qt5/meson.build` never passes
   `KIME_QT_IID`, so the qt5 plugin metadata IID lacks the `.5.1` suffix and
   Qt5 silently loads no kime input context (same class as #736/#756, which
   fixed qt6 only). The test detects the missing IID string in the built
   plugin and skips with a message; fix the meson recipe (mirror
   `qt6/meson.build`) and it runs fully.
4. **`q01_757` SKIPs:** PR #771 guards `setFocusObject` but Qt calls
   `commit()` → `reset()` *first* on focus-out, so the candidate popup still
   dies. The test detects the popup death and skips with a message; guard
   `commit()` with `engine_ready` in
   `src/frontends/qt5/src/input_context.cc` and it runs fully.

Two further residual issues are documented in test doc comments:
`w01_714_no_spurious_commit` (bypassed keys re-send an identical preedit —
asserted at "no bare commit" strength instead of "no commit") and
`g3_02_617_no_zombie_candidates` (Esc in hanja mode is swallowed as a merged
global hotkey and leaks the candidate popup — the test dismisses with
BackSpace instead).
