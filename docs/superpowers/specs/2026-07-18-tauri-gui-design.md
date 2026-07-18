# o4fix Plan 2 — Tauri GUI, packaging, CI — design spec

Date: 2026-07-18
Status: approved (per-section user approval in session)
Repo: https://github.com/ThaumielSparrow/o4fix
Parent spec: `2026-07-14-rust-refactor-design.md` (sections 5–8 remain
authoritative for GUI behavior, error handling, and the Config surface;
§10's packaging/CI intent carries over with two supersessions in §6 below:
the CLI binary is `o4fix.exe` (renamed in Plan 1), and WebView2 uses a
runtime check instead of an installer bootstrapper).

## 1. Context

Plan 1 (Rust port of the MP4-repair pipeline) shipped to `main` on
2026-07-18: `o4core` + `o4fix-cli` (`o4fix.exe`), parity-locked to the
Python reference by seeded goldens and an acceptance render on the M2 row.
Plan 2 delivers the remaining spec scope: the `o4fix-app` Tauri GUI,
portable-zip packaging on GitHub Releases, and GitHub Actions CI — plus
the debt items carried from Plan 1's final whole-branch review.

## 2. Decisions taken this session

- **Parent spec §6/§10 build as-is** — using the CLI for two weeks changed
  nothing about the wanted GUI.
- **fmt**: one-time `cargo fmt` across the workspace, then CI enforces
  `cargo fmt --check` forever. Reformat commit goes in
  `.git-blame-ignore-revs`.
- **M4/anchor validation** (branches never runtime-executed in Rust):
  quat-level parity smoke against seeded Python, not a render eval.
  M4 becomes a permanent clip-gated e2e test; anchor mode a one-off
  documented smoke.
- **Multi-clip validation: explicitly deferred post-release.** v0.1.0
  ships validated on the single test clip; additional-clip reports are
  post-release feedback. (Parent spec's R² < 0.8 error path is the
  safety net for foreign clips.)
- **Sequencing: prep → CI → GUI → release.** CI lands before GUI work so
  the known-fiddly opencv-crate-on-runner risk is retired early and every
  GUI commit is gated.

## 3. Phase 1 — Prep

1. **Format commit.** `cargo fmt` on the whole workspace as a standalone
   commit touching nothing else; add `.git-blame-ignore-revs` containing
   that commit's hash; note `git config blame.ignoreRevsFile
   .git-blame-ignore-revs` in `rust/README.md`. Fast test suite
   (`cargo test -p o4core -p o4fix-cli`) re-run to prove no behavior
   change.
2. **M4 parity smoke → permanent test.** Extend `tools/dump_goldens.py`
   to also produce a seeded Python reference with `--fast-wide-cutoff 16`
   (accel gate at its default 1500) as `goldens/ref_fixed_m4.MP4`. New
   `--ignored` e2e test runs the Rust pipeline with the same Config and
   compares: clean zones bit-exact, repaired zones ≤ 1e-6 sign-folded —
   same harness pattern and tolerances as the existing e2e gate.
3. **Anchor one-off smoke.** Run Python and Rust once with
   `--anchor-mode` (default `--anchor-cutoff 1.5`) on the test clip,
   seeded; diff output quats at e2e tolerances. Documented in the plan's
   task report, not kept as a permanent test (off-by-default,
   experimental path).
4. **Golden-harness restructure.** Replace the
   `#[path = "golden_telemetry.rs"] mod gt;` cross-includes with a shared
   `o4core/tests/common/mod.rs` (subdirectory modules are not compiled as
   separate test binaries), deleting the `#[allow(dead_code)]`
   workaround on `npz_extract`. Pure test-layout refactor; all golden
   tests re-run green afterwards.

## 4. Phase 2 — CI (`.github/workflows/ci.yml`)

Triggers: push and PR to `main`. Runner: `windows-latest`.

- **Job `fmt`** (fast, no OpenCV): checkout, pinned stable Rust,
  `cargo fmt --check`.
- **Job `test`**: pinned stable Rust; OpenCV 4.12.0 installed from the
  official Windows self-extractor release, extracted to `C:\opencv`,
  cached with `actions/cache` keyed on the OpenCV version; libclang from
  the runner image's LLVM (pin via choco only if the image version
  misbehaves — decided at implementation, recorded in the workflow
  comments); env vars exactly as documented in `rust/README.md`
  (`OPENCV_INCLUDE_PATHS`, `OPENCV_LINK_PATHS`,
  `OPENCV_LINK_LIBS=opencv_world4120`, `LIBCLANG_PATH`, PATH additions);
  `Swatinem/rust-cache`; then `cargo clippy --workspace -- -D warnings`
  and `cargo test -p o4core -p o4fix-cli` (layer-1 fixture tests only —
  no clip, no goldens, no `--ignored`). After Phase 3 lands, this job
  also builds `o4fix-app`.
- Clip-gated goldens and byte gates stay local-only; the Phase 4 release
  checklist requires them green before tagging.

## 5. Phase 3 — GUI (`rust/o4fix-app`)

Behavior and layout per parent spec §6 (drop zone, queue rows with status
chips and progress bars, collapsible log pane, advanced drawer with
Default (M2) / Sharp-turn (M4) / Custom profiles, full Config surface
grouped like the CLI argument groups with argparse help text as tooltips,
reset-to-defaults, output-folder override, `concurrent_files` 1–3 default
1, settings persisted to JSON in the app config dir). Error behavior per
parent spec §7 verbatim, including: round-trip verify failure deletes the
output and hard-errors; cancel deletes partial output.

Implementation shape:

- **Tauri 2**; frontend is plain HTML/CSS/JS in `o4fix-app/ui/`
  (`frontendDist` pointing at the folder; no Node, no bundler).
- **Commands**: `pick_files` (native dialog), `start_queue(files,
  config)`, `cancel(job_id)`.
- **Events**: `job_progress {file, stage, pct, detail}`,
  `job_done {file, status}`, `log_line {file, line}`.
- **o4core change (additive)**: `pipeline::Progress` gains a progress
  fraction (`pct`) so the GUI can drive per-file bars; explicitly
  deferred to Plan 2 by the Plan 1 doc. CLI printing is unchanged.
- **Queue**: worker pool honoring `concurrent_files`; one job = one
  file; per-job cancel `AtomicBool` (checked between stages and between
  optical intervals, as already wired in `pipeline::process`); each job
  wrapped in `catch_unwind` so a panic on one file marks that row failed
  and never kills the queue.
- **Stage → chip mapping**: Extract+Analyze → "analyzing", Optical →
  "measuring motion", Splice+Write → "patching", verify → "verifying";
  terminal states done / healthy-nothing-to-repair / error / cancelled.
- Drag-and-drop via Tauri's webview file-drop events; dropped non-MP4
  files are rejected with a row-level error, not a dialog.

## 6. Phase 4 — Packaging + release (`.github/workflows/release.yml`)

- Trigger: tag `v*`. Release build of `o4fix-cli` + `o4fix-app`;
  assemble portable folder: `o4fix-app.exe`, `o4fix.exe`,
  `opencv_world4120.dll`, `opencv_videoio_ffmpeg4120_64.dll`,
  `README.txt`; zip as `o4fix-vX.Y.Z-windows-x64.zip`; publish GitHub
  Release. First release: **v0.1.0** (version synced across
  `Cargo.toml`s and `tauri.conf.json`).
- **WebView2**: portable zip means no installer bootstrapper. Win11
  always has WebView2; on bare Win10 the app checks at startup and shows
  a message box with the Evergreen download link; README.txt carries the
  same note.
- **Release checklist** (committed to the repo, e.g.
  `docs/release-checklist.md`): goldens regenerated, byte gates +
  both e2e goldens (M2, M4) green locally, CI green on the tagged
  commit, zip smoke-tested from a clean folder without dev env vars
  (drop clip → `_fixed.MP4`), README download instructions current.
- Root `README.md` gains pilot-facing sections: what it fixes, download
  link, drop-zone usage, CLI usage, the healthy-clip and
  couldn't-calibrate messages explained.

## 7. Testing & verification

- Phases 1–2 are verified by the existing test pyramid: fast suite in
  CI, clip-gated goldens locally (now including the M4 e2e), byte gates
  unchanged and required green.
- Phase 3: queue/cancel/settings logic that is pure Rust gets unit
  tests (profile→Config mapping, custom-detection, settings round-trip);
  the webview UI itself is verified by a scripted manual smoke checklist
  in the plan (drop file, batch of 3, cancel mid-optical partial-output
  deletion, healthy clip chip, bad-file error isolation, settings
  persistence across restart, M4 profile produces the fast-wide log
  line).
- Phase 4: the zip smoke test from the release checklist is the
  acceptance test for "plug and play" (parent spec success criterion 4),
  run on this machine from a clean shell without the dev env vars.

## 8. Risks & open questions

- **opencv crate on GitHub runners** is the main schedule risk
  (bindgen + libclang + link env). Mitigated by doing it first (Phase 2
  before GUI), pinning OpenCV 4.12.0 exactly as on the dev machine, and
  the documented env-var gotchas in `rust/README.md`.
- **Runner image LLVM drift**: if the preinstalled libclang breaks
  bindgen, pin LLVM via choco at a known version (same failure mode is
  documented from the dev-machine setup).
- **WebView2-absent path** can't be fully tested on this Win11 machine;
  the check is a small guarded code path with a message box — accepted
  risk, README covers manual install.
- **Foreign clips** (post-release): the R² gate and hard verify gate are
  the safety nets; per parent spec the app never ships a corrupt file.
  Multi-clip validation deliberately deferred.

## 9. Success criteria

1. Phase 1: workspace passes `cargo fmt --check`; M4 e2e golden green;
   anchor smoke documented green; golden tests green under
   `tests/common` layout.
2. Phase 2: CI green on `main` (fmt + clippy + fast tests) with cached
   OpenCV; wall time ≤ ~15 min cold, ≤ ~5 min warm.
3. Phase 3: GUI manual smoke checklist fully green; parent spec success
   criterion 5 met (profiles, persistence, queue with per-file progress,
   cancel, error isolation).
4. Phase 4: tagged v0.1.0 produces a zip on GitHub Releases; clean-folder
   smoke test passes (parent spec criterion 4).
