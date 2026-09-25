# Rolling-shutter ablation — September 10, 2026

Diagnostic only: identical default repaired 0060 footage and Gyroflow settings, except frame readout compensation is 0 instead of 5.092569241816712 ms. Both the stabilization override and embedded project lens-profile readout field are set to zero to make the ablation unambiguous. This does not assert that the camera physically has zero readout time.

No new smoothing, optical tracker, handback, telemetry, or rebase setting is introduced. Same automatic zoom settings are retained; their resulting envelope may respond to the readout change. The renderer processes the full clip before excerpts are cut. Baseline is the previously verified default render.

Both renders have 38269 frames at 100 fps, 1440x810, duration 382.69 seconds. A pixel comparison of 245-253 seconds gives SSIM 0.947840: this confirms a real output difference, not an improvement score. The decoded side-by-side layout was inspected.

Comparison clips are in target/experiments/rs-v1: compare_106, compare_225, compare_249, compare_308 (8 seconds each), and compare_clean (19-22 seconds, 3 seconds). Left is normal compensation; right is compensation off. Each panel is 720x405 with a one-pixel bottom pad for encoding. Playback stays at 100 fps. There is no additional fixed zoom as in the retired local smoothing experiment.

Review protocol (user verdict recorded below): does turning compensation off change the persistent wobble/judder, especially during turns, and what happens in the clean control? If clearly worse, retain normal compensation. If clearly better, investigate readout timing and within-frame orientation before any setting change. If nearly indistinguishable, deprioritize this mechanism and return to perspective/parallax-aware motion estimation. Existing spatial results do not by themselves diagnose rolling shutter.

## User verdict

User saw a possible slight improvement with compensation off, but the difference was close and not drastic. Classify as weak/uncertain visual benefit, not a demonstrated fix. Keep the existing compensation setting. Deprioritize readout compensation as the main cause, without claiming it has been conclusively ruled out. Resume perspective-aware motion estimation with persistent tracks and regional agreement checks.
