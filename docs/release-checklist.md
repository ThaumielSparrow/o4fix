# Release checklist

CI cannot run the clip-gated tests (1.7 GB clip + Python goldens), so a
release REQUIRES this local gate first. All on the release commit:

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   fast suite `cargo test -p o4core -p o4fix-cli -p o4fix-app` — green.
2. Goldens present (else `python tools/dump_goldens.py`, ~25-30 min incl. M4).
3. Full clip-gated suite green: `cargo test -p o4core -- --ignored`
   (extraction, detect, mp4 byte gates incl. nullpatch/inject, optical,
   fit, patch, splice, e2e M2 **and** e2e M4). ~45-60 min.
4. GUI smoke (plan Task 9 checklist) passed on this commit.
5. Versions synced: `rust/o4fix-cli/Cargo.toml`, `rust/o4fix-app/Cargo.toml`,
   `rust/o4fix-app/tauri.conf.json` all equal the tag (minus the `v`).
6. CI green on `main` for the commit being tagged.
7. `git tag vX.Y.Z && git push origin vX.Y.Z`; watch the Release workflow.
8. Download the published zip; smoke it from a CLEAN environment
   (PATH stripped to System32 so the zip's own DLLs must resolve):
   `cmd /c "set PATH=C:\Windows\System32;C:\Windows&& o4fix.exe <clip> -o <tmp out>"`
   then launch `o4fix-app.exe` the same way and repair a clip via the GUI.
9. Sanity-check the README download link still points at the release.
