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
- `o4fix-cli/` — command-line interface (stub; Task 16 replaces with full CLI)

## Dev commands

```bash
cd rust
cargo build                   # Build all crates
cargo test -p o4core         # Run o4core unit tests
cargo test -p o4core -- --ignored  # Run integration tests (requires test clips)
```

## OpenCV setup

Filled in by Task 11 (local OpenCV install required for optical.rs).

## telemetry-parser pin

**Status: validated (Task 3 spike).** Rust crate output is byte-identical
to the installed Python bindings' output on the real test clip
(`sample_vids/DJI_20260711124046_0021_D.MP4`, 352,736 flat quat-stream
rows, sha256 `8964bfea733954cd0ba2c7b1ccc4c277cd3750c757de02e527b64abcdfaf3300`
on both sides). See `rust/o4core/examples/dump_quats.rs` /
`tools/dump_quats_py.py` (untracked CSVs; do not commit them).

### Pin

```toml
# rust/o4core/Cargo.toml
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
Rust's `e3`/`e-4`) — `tools/dump_quats_py.py`'s `_rust_exp()` re-renders
to Rust's convention before comparing, and Python's stdout also needs
`reconfigure(newline="\n")` on Windows or it CRLF-translates and breaks
the byte diff. Neither adjustment changes any data value; both are
documented inline in that script.
