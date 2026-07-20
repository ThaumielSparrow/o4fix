# o4fix Rust implementation

## Layout

- `o4core/` — core library
  - `config.rs` — tuning parameters (Config struct, defaults M2 profile)
  - `error.rs` — error types (O4Error enum)
  - `dsp.rs` — signal processing (butter, lfilter, filtfilt, kernels)
  - `quat.rs` — quaternion algebra (exp, log, slerp)
  - `telemetry.rs` — telemetry parsing from DJI MP4 metadata
  - `mp4.rs` — MP4 box walk and patching primitives
  - `detect.rs` — noise detection (adaptive_clean, find_intervals)
  - `optical.rs` — video-based motion estimation (video_rates, fit_video_alignment)
  - `patch.rs` — quaternion rewriting (optical_patch, splice_orientation)
  - `pipeline.rs` — main processing pipeline (process)
- `o4fix-cli/` — command-line interface (`o4fix.exe` binary; `src/args.rs`
  is a clap-derive `Cli` mirroring `o4fix.py`'s argparse block field-for-field,
  `src/main.rs` drives `o4core::pipeline::process` over each video)
- `o4fix-app/` — Tauri 2 GUI ("o4fix-app.exe"; plain HTML/CSS/JS in ui/,
  no Node build step; settings persist to
  %APPDATA%\com.thaumielsparrow.o4fix\settings.json)

## Dev commands

Run all cargo commands from the repo root (the Cargo workspace lives there).

```bash
cargo build                   # Build all crates
cargo test -p o4core         # Run o4core unit tests
cargo test -p o4core -- --ignored  # Run integration tests (requires test clips)
cargo run -p o4fix-app        # Run the GUI in debug (devtools via right-click → Inspect)
cargo build --release -p o4fix-app  # Build the GUI in release
```

### Formatting

The workspace is rustfmt-clean and CI enforces `cargo fmt --check`.
The initial mechanical reformat is listed in `.git-blame-ignore-revs`;
run `git config blame.ignoreRevsFile .git-blame-ignore-revs` once locally
so `git blame` skips it.

### Goldens

`cargo test -p o4core -- --ignored` (and the individual `--test golden_*`/
`--test e2e` runs) compare against Python-generated golden fixtures that
are NOT checked in. Before running any `--ignored` test, generate them
from the repo root:

```bash
python python/tools/dump_goldens.py   # ~10-20 min; writes untracked goldens/,
                                # including the seeded reference goldens/ref_fixed.MP4
```

`dump_goldens.py --m4-only` regenerates only `goldens/ref_fixed_m4.MP4`
(M4-profile e2e reference; the full run now writes both).

## o4fix-cli (Task 16)

`o4fix-cli` is a clap-derive CLI over `o4core::pipeline::process`, ported
from `o4fix.py`'s MP4-repair mode only. Flag names, help text, and every
tuning default are copied field-for-field from `o4fix.py`'s `argparse`
block (`--severe 8`, `--noise-band 30 180`, `--fast-wide-accel 1500`,
etc. — see `src/args.rs`'s `Cli::to_config()` for the full mapping onto
`o4core::config::Config`). The legacy gcsv pipeline stays Python-only per
spec: `--gcsv`, `--plot`, `--orientation`, `--lpf`, `--no-optical` are
intentionally NOT ported, so passing any of them errors out via clap's
"unknown flag" handling — that error is the correct/intended behavior,
not a bug.

Build/run:

```powershell
cargo build -p o4fix-cli --release
cargo test -p o4fix-cli                 # 3 CLI-parsing tests, no clip needed
./target/release/o4fix.exe VIDEO.MP4 -o OUT.MP4
```

**DEVIATION (exit codes):** `o4fix.py` always exits 0, even when
`process(v, args)` fails for one of several input videos (Python's
`main()` never checks a return value or catches per-video exceptions
into a failure flag). `o4fix-cli` exits 1 (`std::process::ExitCode::FAILURE`)
if *any* video in the batch fails, and 0 only if all succeed — better for
scripting/CI, and it's what `process()`'s `Result` return already gives
us for free. Usage errors (bad flags, `-o` with multiple videos) exit 2
in both implementations (argparse's `p.error()` / clap's default `Error::exit()`).
Console message text and order match Python's, with the one exception
already noted in `pipeline.rs`'s doc comment (Task 15 deliberate reorder:
the severe-burst-count line prints before optical progress, not after).

## OpenCV setup

**Status: done (Task 11).** `optical.rs` needs a local OpenCV 4.12.0 install
(matching the `cv2` version the Python reference/goldens were generated
with — decode parity requires the exact same C++ OpenCV version) plus
LLVM/libclang for the `opencv` crate's bindgen step.

### What was installed on this machine

1. **LLVM 22.1.8** via `winget install -e --id LLVM.LLVM --accept-source-agreements --accept-package-agreements`
   → `C:\Program Files\LLVM\bin\libclang.dll`.
2. **OpenCV 4.12.0** — downloaded
   `https://github.com/opencv/opencv/releases/download/4.12.0/opencv-4.12.0-windows.exe`
   to `%TEMP%` (curl.exe, ~187 MB), ran `opencv-4.12.0-windows.exe -oC:\ -y`
   (7-zip self-extractor) → `C:\opencv\build\...`. Note: the extractor
   returns before all files are flushed to disk — if you `Test-Path` the
   `bin` DLLs immediately after the `&` call returns and see nothing, wait
   a couple seconds and recheck before concluding the extraction failed.
3. `cargo add opencv --package o4core` → `opencv = "0.99.0"` in
   `o4core/Cargo.toml` (default features; pulls in `calib3d`/`imgproc`/
   `video`/`videoio`/`core` modules, which is all `optical.rs` needs).

### Required environment variables

```
OPENCV_INCLUDE_PATHS = C:\opencv\build\include
OPENCV_LINK_PATHS    = C:\opencv\build\x64\vc16\lib
OPENCV_LINK_LIBS     = opencv_world4120
LIBCLANG_PATH        = C:\Program Files\LLVM\bin
PATH                += C:\opencv\build\x64\vc16\bin   (runtime DLLs, incl. opencv_videoio_ffmpeg4120_64.dll)
PATH                += C:\Program Files\LLVM\bin      (runtime libclang.dll — see gotcha below)
```

These are persisted user-level (`setx` / `[Environment]::SetEnvironmentVariable`
with `'User'` scope) but **do not propagate to already-open shells** — set
them inline in the same command for any `cargo build`/`test` invocation in
a session that predates the persist, e.g.:

```powershell
$env:OPENCV_INCLUDE_PATHS='C:\opencv\build\include'; $env:OPENCV_LINK_PATHS='C:\opencv\build\x64\vc16\lib'; $env:OPENCV_LINK_LIBS='opencv_world4120'; $env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'; $env:PATH="$env:PATH;C:\opencv\build\x64\vc16\bin;C:\Program Files\LLVM\bin"
cargo build -p o4core
```

### Gotchas hit during setup (both cost real time — save the next person)

1. **`LIBCLANG_PATH` alone is not enough for `PATH`.** The `opencv` crate's
   build script (`clang-sys`, non-`runtime` linking as configured here)
   hard-links `libclang.dll` at load time, not just at build.rs discovery
   time. Without `C:\Program Files\LLVM\bin` also on `PATH`, the build
   script binary itself fails to *start* with `STATUS_DLL_NOT_FOUND`
   (`0xc0000135`, cargo reports "process didn't exit successfully ...
   exit code: 0xc0000135") — a confusing error that looks like a link
   failure but is actually the loader failing to find `libclang.dll` for
   the build-script executable. Fix: add the LLVM bin dir to `PATH`, not
   just `LIBCLANG_PATH`.
2. **`setx` silently truncates values over 1024 characters.** Running
   `setx PATH "$oldPath;C:\opencv\build\x64\vc16\bin"` on a `PATH` that's
   already near/over the limit truncates the stored value mid-entry and
   prints `WARNING: The data being saved is truncated to 1024 characters.`
   — easy to miss, and it silently drops trailing PATH entries from the
   user environment. Use `[Environment]::SetEnvironmentVariable('PATH', $value, 'User')`
   instead (no length limit); if you must recover a value already
   truncated by `setx`, the still-running shell's `$env:PATH` (inherited
   at process start, unaffected by later registry writes) plus
   `[Environment]::GetEnvironmentVariable('PATH','Machine')` let you diff
   out the pre-truncation user-scope entries.
3. The opencv crate ships its generated Rust bindings per-build into
   `target/debug/build/opencv-<hash>/out/opencv/*.rs` — when the exact
   arg count/name of a wrapped OpenCV function is unclear (multiple
   `_def`/numbered overloads for C++ default arguments), grepping that
   generated output is faster and more reliable than guessing from
   docs.rs, since it reflects the exact opencv crate version + OpenCV
   version pair actually linked.

### API notes for `optical.rs` vs. the Python reference (`opencv` crate 0.99.0 / OpenCV 4.12.0)

- `cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)` → `imgproc::cvt_color_def(src, dst, code)`
  (3-arg form; the base `cvt_color` in this crate version takes 5 args
  including an explicit `dst_cn`/`AlgorithmHint`, which the Python call
  doesn't specify — `_def` matches the Python defaults exactly).
- `cv2.fisheye.undistortPoints(pts, K, D)` (no R/P) →
  `calib3d::fisheye_undistort_points_def(distorted, undistorted, k, d)`
  (4-arg form; the base `fisheye_undistort_points` in this crate version
  requires explicit `r`/`p`/`criteria` args).
- `Mat::from_slice(&d)` returns `Result<BoxedRef<'_, Mat>>` (borrows the
  input slice) — since the distortion coefficients are a local stack
  array in `k_d()`, this needs `.try_clone()` to get an owned `Mat` that
  can be returned from the function.
- `good_features_to_track`, `calc_optical_flow_pyr_lk`, `find_essential_mat`
  (8-arg: points1/points2/camera_matrix/method/prob/threshold/max_iters/mask),
  `decompose_essential_mat`, `Rodrigues`, `TermCriteria::new`, `Mat::at::<T>`
  matched the brief's call sites exactly (verified against the generated
  bindings) — no changes needed there.

## dsp.rs: sci-rs evaluated and rejected (Task 5)

**Status: hand-ported (Task 5).** Per the task brief's decision gate,
`sci-rs = "0.4.1"` (latest on crates.io, `std` feature) was added as a real
dependency and probed against `tests/fixtures/filters.json` (scratch
example, not committed) before writing any hand-port code. Result: **gate
failed**, sci-rs removed (`cargo remove sci-rs`), Step 4 hand-port used
instead.

Two independent failures, either one sufficient on its own:

1. **No Ba-format `lfilter`/`filtfilt` at all.** sci-rs only exposes
   `sosfilt`/`sosfiltfilt` (second-order-sections). The brief's `dsp::`
   interface requires `lfilter(&Ba, ...)` / `filtfilt(&Ba, ...)` operating
   directly on single b/a coefficient vectors (matching scipy's default
   `output='ba'`, which is what the fixture was generated with — e.g. the
   bandpass case has one 5-tap `b`/`a` pair, not per-section SOS state).
   There is no upstream code path to wrap for 2 of the 4 required
   algorithms — the DF2T `lfilter` and odd-extension-padded `filtfilt`
   would have to be hand-written regardless of the sci-rs decision.
2. **`butter_dyn(..., FilterOutputType::Ba)` shape mismatch.** For a
   lowpass/highpass filter of order N, sci-rs pads `b`/`a` to length
   `2N+1` (the bandpass-shaped length) with trailing exact-zero
   coefficients, instead of scipy's minimal length `N+1`. E.g. for
   `butter_low(2, 0.05)`: sci-rs returns `b.len()==5, a.len()==5`
   (`b=[..., ..., ..., 0.0, 0.0]`) vs. the fixture's `b.len()==3`. The
   fixture's `close()` helper (and scipy) expect the trimmed length; this
   would need extra wrapper-side trimming logic on top of an already-thin
   wrapper.

Where shapes did match (the one bandpass case, order 2, `wn=[0.06,0.36]`),
the *numeric* values were excellent — `b`/`a` max relative error
`5.8e-16`, `lfilter_zi` (via `lfilter_zi_dyn`, which does exist for Ba)
`9.4e-16` — well inside the 1e-12/1e-10 bars. So sci-rs's underlying
zpk/bilinear math is sound; it's the API surface (SOS-only filtering,
untrimmed Ba design) that doesn't meet "all fixture cases pass at stated
tolerances out of the box."

**Hand-port result:** all 6 fixture cases (5 lowpass wn, 1 bandpass) pass
at the brief's tolerances (b/a ≤1e-12, zi ≤1e-10, lfilter/filtfilt ≤1e-9);
measured max relative errors are all ≤~4.6e-14 (machine-epsilon range),
see `.superpowers/sdd/task-5-report.md` for the per-case table. The
`dsp::` public API (`Ba`, `butter_low`, `butter_band`, `lfilter_zi`,
`lfilter`, `filtfilt`, `filtfilt_padlen`, `filtfilt3`) is exactly the
brief's Step 4 code, with one intentional deviation: `lfilter_zi`'s
companion-transpose loop is written directly to the *documented correct*
definition (`Ct[i][jj] = -a[i+1]/a[0] when jj==0; = 1 when i==jj-1; else
0`) rather than the brief's deliberately-buggy Step 4 snippet — verified
against the bandpass fixture case, which is the one case sensitive to
this trap (lowpass zi is 1-D and can't distinguish the two index
conventions).

## telemetry-parser pin

**Status: validated (Task 3 spike).** Rust crate output is byte-identical
to the installed Python bindings' output on the real test clip
(`sample_vids/DJI_20260711124046_0021_D.MP4`, 352,736 flat quat-stream
rows, sha256 `8964bfea733954cd0ba2c7b1ccc4c277cd3750c757de02e527b64abcdfaf3300`
on both sides). See `o4core/examples/dump_quats.rs` /
`python/tools/dump_quats_py.py` (untracked CSVs; do not commit them).

### Pin

```toml
# o4core/Cargo.toml
telemetry-parser = { git = "https://github.com/AdrianEddy/telemetry-parser", rev = "4abe30846be4da7d4e9dbbb55002f6d9cfd86ae5", version = "0.3.0" }
```

`rev` = the commit tagged `v0.3.0` ("Release v0.3.0", `4abe308`). Note:
`git rev-parse v0.3.0` returns `0863154...` — that's the *annotated tag
object*'s own SHA, not a commit; `git cat-file -p 0863154` shows `object
4abe3084...`/`type commit` underneath. `cargo add` initially wrote the
tag-object SHA as `rev` and it happened to work (Cargo peels tags to
commits, and `Cargo.lock`'s `source` URL recorded the resolved commit
after the `#`: `...rev=0863154...#4abe3084...`), but pinning the actual
commit hash directly is clearer and more conventional, so that's what's
in `Cargo.toml` now. Rebuilt and re-ran the full parity check after the
switch — same sha256 as before. Chosen deliberately over `master` HEAD
(`77a3b810a0e0f64688a90546c5aaf24c9dba00bd` as of 2026-07-16): the
installed Python package is `telemetry-parser==0.3.0`
(`pip show telemetry-parser`), i.e. built at/near the same release, while
`master` has ~90 commits of drift since the tag (new camera formats,
`DeviceMultiAttitude`, oq101/wa530 auto-detection, dependency bumps).
Diffed `src/dji/` between the tag and `master`: `dvtm_wm169.proto` is
byte-identical; the generated `dvtm_wm169.rs` differs by thousands of
lines but only in prost-build cosmetics (comment reflow, `tag="1"` ->
`tag = "1"`, added `Copy` derives, unrelated new messages) plus new
protobuf variants for other DJI models — the wire-relevant shapes for our
path (`DeviceAttitude`, `Quaternion`, `FrameMetaOfImu.imu_attitude_after_fusion`
tag 2) are unchanged. `mod.rs`'s wm169 decode branch (multiply_quats,
the Y-axis-180 rotation, the norm-jump sign-continuity flip, the vsync
timestamp formula) is textually identical on both sides of the diff, just
now wrapped in a `handle_parsed!` macro that also serves the newer
formats. So `master` would very likely also pass this parity check for
O4P clips, but the release tag is the more defensible, actually-tested
choice — don't casually bump this rev without re-running the spike.

The transitive `mp4parse` dependency resolves from crates.io
(`mp4parse = "0.17.0"`, registry source) at this rev, not git — so there
is no second git rev to track for it.

### Discovered API (what Task 7's `telemetry.rs` codes against)

```rust
use telemetry_parser::Input;
use telemetry_parser::tags_impl::*;   // GroupId, TagId, TagValue, TagMap, GroupedTagMap, ...

let input = Input::from_stream(
    &mut file, size, &path,
    |_progress: f64| (),
    Arc::new(AtomicBool::new(false)),
)?;                                    // -> Input { samples: Option<Vec<SampleInfo>>, .. }

for sample in input.samples.as_ref().unwrap() {   // one per metadata-track frame
    let Some(group_map) = sample.tag_map.as_ref() else { continue };  // Option<GroupedTagMap>
    let Some(tag_map) = group_map.get(&GroupId::Quaternion) else { continue };  // GroupedTagMap = BTreeMap<GroupId, TagMap>
    let Some(desc) = tag_map.get(&TagId::Data) else { continue };     // TagMap = BTreeMap<TagId, TagDescription>
    let TagValue::Vec_TimeQuaternion_f64(vt) = &desc.value else { continue };  // TagDescription.value: TagValue
    for q in vt.get() {                // ValueType<T>::get(&self) -> &T : here &Vec<TimeQuaternion<f64>>
        // q.t: f64 (ms, relative to first frame); q.v: Quaternion<f64> { w, x, y, z }
    }
}
```

Notes / corrections vs. the brief's sketch (the brief's shape was a
starting guess — this is what the pinned rev's source actually has):
- **`TagMap::get_t::<T>(TagId) -> Option<&T>`** exists (via the
  `GetWithType<T>` trait) and returns the *unwrapped* concrete type
  directly (e.g. `&Vec<TimeQuaternion<f64>>`), **not** `Option<&TagValue>`.
  The brief's `if let Some(TagValue::Vec_TimeQuaternion_f64(arr)) =
  grp.get_t(TagId::Data)` does not type-check against that signature.
  `dump_quats.rs` instead uses the plain `BTreeMap::get` + explicit
  `TagValue` match shown above, which does compile and is what makes
  `arr.get()` (a `ValueType::get()` call, not `Vec::get`) meaningful.
- `GroupedTagMap` / `TagMap` are plain `BTreeMap` type aliases
  (`tags_impl::GroupedTagMap`, `tags_impl::TagMap`), not opaque types.
- `TimeQuaternion<f64>` has fields `t: f64` (milliseconds) and
  `v: Quaternion<f64>` (fields `w, x, y, z`, all `f64`); this matches the
  brief exactly.
- No feature flags are needed for DJI/O4P parsing — it's compiled in
  unconditionally. The crate's only optional feature is `sony-xml`
  (irrelevant here). `build.rs` is a no-op (the DJI `.proto` files are
  pre-generated into checked-in `.rs` — no `protoc` install required).
- DJI sub-format auto-detection (wm169 vs. wa530 vs. oq101) scans the
  first 64 bytes of the first metadata sample for `"oq101"` / `"WA530"`
  / `"wa530"` markers, defaulting to wm169 — matches CLAUDE.md's
  established fact that O4P carries no such marker.

### Flat quat-stream CSV format (for future golden fixtures)

`t_ms,w,x,y,z`, one row per raw emission-order slot (2 kHz slots, each
1 kHz sample duplicated — no sort, no dedup at this layer; that happens
downstream in `o4fix.extract_quats` / Task 7's `telemetry.rs`), full
precision (18 significant digits: `{:.17e}` in Rust). Python's `%.17e`
renders the identical mantissa digits but a different exponent
convention (always-signed, >=2-digit-padded, e.g. `e+03`/`e-04` vs.
Rust's `e3`/`e-4`) — `python/tools/dump_quats_py.py`'s `_rust_exp()` re-renders
to Rust's convention before comparing, and Python's stdout also needs
`reconfigure(newline="\n")` on Windows or it CRLF-translates and breaks
the byte diff. Neither adjustment changes any data value; both are
documented inline in that script.
