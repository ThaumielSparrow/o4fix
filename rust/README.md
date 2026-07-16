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

Filled in by Task 3 (version pinned after spike validation).
