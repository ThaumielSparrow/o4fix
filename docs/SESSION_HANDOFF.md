# Stabilization research handoff

Last updated: 2026-09-24. This document supersedes stale “work paused / solved” status in historical notes. Start here when resuming in a new session.

For the complete experiment-by-experiment index and consolidated user rulings, start with [Stabilization experiments and rulings handoff](STABILIZATION_EXPERIMENTS_HANDOFF.md). That ledger was compiled from every experiment report and the recorded visual verdicts; it supersedes older summary/status wording below when entries conflict. This file retains the production invariants, artifact locations, runtime notes, and historical investigation trail.

## Production promotion (2026-09-24): v0.1.3 candidate with residual refinement

Branch `refine-v013` (HEAD a1e551e, not merged, not released). `o4core/src/refine/` ports the wp1c Gyroflow-free probe and correction into the repair pipeline, and it is on by default. `--no-refine` or the GUI "Refine residual judder" toggle turns it off. Settings v2 migrates the 0.3 s ramp to 0.19 s. The splice now carries the fixed edge offsets. Bursts are skipped when the correction exceeds 4°, the window is unmeasurable, or the burst is within 0.5 s of the clip start. The results are in the production section of [feedback-v1 results](experiments/feedback-v1/results.md):
- Parity with wp1c: 0.024/0.046/0.010° (0073/0060/0071). The splice reproduces edgeoffset.MP4 exactly.
- Runtime: refinement roughly doubles repair time (0060: 231 s vs 112 s).
- 0021 regression render vs shipped v0.1.2: clean/mild are within noise, and refined bursts improve. The severe/flick shake increases come from the clip-end landing and from cap-skipped bursts where the applied corrections are provably identical, so they are tracker noise.
- Review page (release vs wp1c on 0073/0060; release vs v0.1.2 on 0021): `target/experiments/release-v013/review-release.html`. User verdict pending. Release/tag only on user request.

## Latest (2026-09-24): render-feedback telemetry refinement — POSITIVE user verdict

[feedback-v1](experiments/feedback-v1/results.md) measures residual motion in the Gyroflow render of the accepted edgeoffset repair, maps it to body-frame telemetry error via a perturbation-render system identification (R² 0.95–0.99, lag −2 ms), subtracts Gyroflow's exported intended path, high-passes at 1 Hz, and corrects the repaired quaternions inside severe bursts only (world-frame carry, so everything outside the bursts renders identically: ≤0.02°). On 0073, 1–8 Hz in-burst wobble falls 38–47% in all 7 bursts; 8–50 Hz mixed/flat; controls unchanged. A second iteration does not improve further. Review `target/experiments/feedback-v1/compare_{early,late,control}{,_full,_detail}.mp4`, LEFT edgeoffset (current best validated), RIGHT fb1w. User verdict (2026-09-24): “judder is improved quite a bit. i can still see a tiny amount on the right but its far less noticable than the left.” Generalized to 0060/0071 (fb1) and a gain/timing-corrected second step `fbn` (in-burst 1–8 Hz −25..−62% vs edgeoffset on all 14 0073/0060 bursts); see the report update. 0071 fbn render interrupted (low memory). No promotion yet. User verdict (2026-09-24, fbn): “fbn looks better in most of these clips. its near perfect for all except 0060_308, which i think there is a slight regression, but its very subtle. overall still a win.”

**Gyroflow-free direction (2026-09-24, pending review):** user asked to pursue an in-process replacement for the render loop. `warp_residual_probe` measures the telemetry residual from source frames (fisheye + RS row-time rotation, no rendering). One-pass 0073 correction `wp1c` improves 6/7 bursts under the render measurement; weak on 135.8 s. Render-loop "gain" was a render-measurement scale error (adaptive zoom), not Gyroflow behaviour. Review page: `target/experiments/feedback-v1/review-0073-fbn-vs-wp1c.html` (LEFT fbn, RIGHT wp1c). See feedback-v1 report.

**Review workflow (user request 2026-09-24):** for any visual A/B review, generate a local HTML page embedding all comparison videos (docs/experiments/feedback-v1/review_page.py SPEC.json) and give the user that one file; do not ask them to browse folders.

## Fresh-session starting point (authoritative)

The user requested this handoff to start a fresh session. Research and exports from the last session are complete; no render, test, or repair process needs resuming. No production promotion, commit, or shipping rebuild was performed. Preserve the entire current working tree and all source/reference clips.

**Latest user verdict (render-selective-v1, 2026-09-15):** “close-up looks noticably better. full frame looks better too. still a small bit of vibration/judder left but a visible reduction.” Confirmed visible localized improvement in both views; small residual remains. Freeze this candidate as the strongest visually supported localized post-render correction, without production promotion. No new control/additional or earlier-burst verdict. Earlier v1 translation improvement remains preserved as its reference.

There are five distinct states:

- **Shipping:** reviewed M2 default / optional M4, unchanged by research.
- **Strongest validated research baseline:** fixed edge offsets + 0.19 s ramp + original 0.20 s severe padding + original 30 deg/s rebase gate + 1.5 deg/s decay. Significant visual improvement on 0060 and positive additional validation on 0071/0073.
- **Latest incremental candidate:** same settings plus two-frame noisy optical measurements, retaining one-frame calibration. Built/rendered on 0073 only in this combined form. User sees a small improvement; keep it available alongside the baseline, without silently promoting it.
- **Latest visually supported localized post-render candidate:** render-selective-v1 adds gated residual rotation atop v1 translation for141.253–144.451s. Both close-up and full-frame views visibly improve, with small residual judder. Freeze for unseen-burst validation; not production-ready.
- **Earlier reviewed optical trial:** subset-ensemble-v1 adds equal averaging of disjoint feature-subset rotations to gap2-edge, retaining full-fit fallback and original repair settings. Clean scores improve modestly in 14/16 sections, but the user reports only a possible tiny improvement and essentially identical late-clip judder. Preserve without promotion; other windows remain unjudged.

Start with [bounce localization and combined gap2 results](experiments/bounce-localize-v1/results.md), [negative extra-rebase ablation](experiments/residual-stage-v1/results.md), and [successful edge-offset repair](experiments/gyro-trace-v1/results.md). Historical “next” sections below document prior stages; they do not override this starting point or the current next-step section.

**Latest resumed audit:** [residual-exposure-v1](experiments/residual-exposure-v1/results.md) joins exact source-snapped weights to existing 0073 localization. No detector-flagged noisy samples retain direct raw-orientation ramp weight in either review window. However, light-gyro rate mixing can return to 100% during alpha dips inside a zero-trust rebased interior (142.50/143.25 s); this is separate from fast handback, which is zero there. Patch-minus-optical RMS reaches 8.12/6.83 deg/s in those bins. The 143.00 bin remains optical-only with visible-motion proxies over 1 s from ramps, so gyro leakage is not established as the universal cause. See the report for a bounded partner-only ablation and required controls. No new repair/render, promotion, or shipping rebuild occurred. Do not repeat forced rebasing, zero decay, blanket output smoothing, or gap2 comparison without a new discriminating hypothesis. Lens/scaling and geometric-estimator expansion stay deferred at the user's request.

**Latest follow-up — no meaningful late-clip benefit:** [partner-gate-v1](experiments/partner-gate-v1/results.md) tests light-gyro re-entry on 0073, using accepted one-frame optics and fixed-edge splice. It suppresses the outer light-gyro partner inside zero-trust severe intervals with 0.19 s smooth transitions, retaining internal fast handback. Three tests and Clippy pass; accepted-cache parity, rate-mixture reconstruction, optical coverage and MP4 writeback pass. All six rebase decisions remain unchanged; clean-control rate difference is 0.0123 deg/s. Full render and four comparisons are complete: `target/experiments/partner-gate-v1/compare_{late,early,control,additional}.mp4`. LEFT accepted edgeoffset, RIGHT partner suppression. User sees little/no meaningful late-clip improvement; no other clip verdict. Preserve as a negative late-window experiment, without production promotion/rebuild. Next investigate optical measurement reliability with clean controls and existing observations; do not repeat partner-gate sweeps or automatically combine it with gap2. Lens/scaling and geometric-estimator expansion remain deferred.

## Current active instruction and experiment (2026-09-15)

User explicitly requests: continue exploring until a material render improvement is found, then show the visual. Do not end merely with another negative trial. That exploration produced render-selective-v1, now confirmed visibly better in both presented views. User reports165Hz display and100fps recording, strongly doubts playback cadence and attributes burst failures to a poorly isolated/over-sensitive gyro. Keep focus on corrupt gyro/repair residuals; playback-cadence investigation is set aside.

[render-rotation-v1](experiments/render-rotation-v1/results.md) completes matched translation/rotation diagnostics and a bounded rotation-only correction atop v1. Late high-band roll improves about42%, but additional high-band translation worsens17%; withheld as mixed, no visual verdict or promotion. All rotation renders/probes complete. See report for preflight refusal and exact final40ms/4–30Hz settings.

Regional and2–8Hz translation trials are now complete and rejected as mixed/negative. See [render-regional-v1](experiments/render-regional-v1/results.md) and [render-bounce-v1](experiments/render-bounce-v1/results.md). No output shown to user. A band-specific diagnostic finds late2–4Hz regional transfer+20.74%, but the actual2–8Hz correction worsens low-band translation. Do not present these trials as success.

**Latest positively reviewed visual: [render-selective-v1](experiments/render-selective-v1/results.md).** All rendering and verification complete; no process needs resuming. Selective rotation atop visually supported v1 translation acts only inside the141.253085–144.451232s existing gyro-corruption burst with200ms internal fades. Held-out regional benefit plus at least5 half-second blocks selects this event; other late events and control/additional abstain. This eligibility rule is exploratory on known windows, not production-validated. No lens/source gyro/pose estimation edits, promotion, commit or shipping rebuild.

Final encoded localized result:2–8Hz roll -46.13%,8–30Hz roll -88.53%; translation+1.27%/-0.85% over141.25–144.5s. Whole-window metrics mixed (high-band translation+8.62%). Zero-correction controls themselves have nonzero optical-proxy differences between encoded panels, demonstrating encoding/measurement variability; do not overinterpret small percentages. User confirms both close-up and full-frame views visibly improve, with small residual vibration/judder. This is a localized candidate, not a complete fix for earlier135–140s judder.

Review `target/experiments/render-selective-v1/compare_detail.mp4` (native-pixel upper-right crop, LEFT previous v1 translation / RIGHT selective correction) and `compare_focus.mp4` (full-frame comparison): both140.5–145.5s,500frames at100fps. Full14s late and control/additional comparisons also retained. Both panels common1.06zoom; disclose extra crop relative to prior1.04. Max rotation0.4418deg; source margin12.96px; angular correction exactly zero outside accepted burst and in controls. Final tests/Clippy pass,26video frame-count/cadence checks and gate/source-margin checks pass, detail layout inspected. Reports/manifests cover all new trials. Next freeze the visually supported candidate and validate the unchanged eligibility rule and transitions on unseen bursts/clips before integration. Characterize the remaining small residual using this reference; do not blindly increase strength. Preserve unresolved shorter/earlier events and keep production unchanged.


## Latest completed experiment — full-resolution residual audit (2026-09-14)

[render-acceleration-v2](experiments/render-acceleration-v2/results.md) keeps 40 ms smoothing fixed and compares prior v1 corrections applied directly to the original 1440x810 render against corrections measured at full resolution. All three comparisons and probes completed. Full-resolution tracking does not materially improve late high-frequency translation (+0.96% versus v1); low-band translation improves 4.55%, with mixed roll. Keep visually supported v1; no v2 promotion or visual verdict.

After the prior v1 correction at full resolution, a shared translation fails held-out regions (-8.83% energy reduction). A read-only combined translation/rotation acceleration field explains 8.41%; clean/additional controls are negative (-19.92/-11.82%). This motivates a matched-estimator rotation diagnostic, not automatic roll correction: existing translation uses a median and combined field least squares, so add a least-squares translation-only control before attributing gains specifically to rotation. Much residual remains spatially inconsistent; do not simply strengthen translation smoothing. No physical-cause attribution or deferred lens/scaling/source-geometric expansion.

Artifacts: `target/experiments/render-acceleration-v2/compare_{late,control,additional}_full.mp4`, plus half-size previews without `_full`; LEFT full-resolution realization of prior v1, RIGHT newly measured full-resolution correction. Seven tests (including a textured-image pan/shake tracking control), release Clippy/build, all frame counts and inverse-warp source margins pass. Source/output SHA256 provenance and diagnostics in the report directory. No pending processes, production changes, promotion or shipping rebuild. This next step supersedes older next-step sections below.

## Latest experiment — rendered acceleration (2026-09-14)

[render-acceleration-v1](experiments/render-acceleration-v1/results.md) follows the user's request for higher-confidence detection/removal of post-Gyroflow judder. Same-feature three-frame acceleration on previous gap2-edge output predicts 34.11% of late held-out feature energy and 20.28% of held-out region energy; clean region transfer is negative. A bounded translation-only 40 ms correction, with common 4% zoom, reduces actual encoded late 8–30 Hz translation 39.53% and shared acceleration 55.18%, without increased roll proxy. However additional 264–272 s high-band translation/roll increase 4.35/25.67%; unconditional use fails. Clean low-band roll rises slightly in absolute terms. Early candidate refused six unsupported triplets. All failures and the rejected 100 ms preflight are documented.

Review ready: `target/experiments/render-acceleration-v1/compare_{late,control,additional}.mp4`, LEFT previous gap2-edge with common crop, RIGHT triplet-acceleration translation correction. Late priority 10.5–11.75 s. User confirms visible partial late improvement, still noticeable judder; additional possibly slightly worse but not significantly so. Clean control unjudged. Five mathematical synthetic tests, release Clippy/build, decoded frame counts and layout checks pass. No process remains running. Production/baseline unchanged, no promotion or shipping rebuild. Next: characterize the remaining acceleration/roll/spatial disagreement in the corrected late output at original resolution, then pair a targeted correction with a spatial-coherence/support gate validated on held-out footage. A lower numerical score alone does not meet the smooth/flowy objective. Do not increase smoothing strength or deploy unconditionally; keep lens/scaling/geometric expansion deferred. This is the current next step, superseding older next-step paragraphs below.

## User objective and current decision

**Latest subset investigation and reviewed trial:** [subset-stability-v1](experiments/subset-stability-v1/results.md) completed 9981 pairs with unchanged full-fit parity and four synthetic tests. Independent feature-subset disagreement is weak/inconsistent as an error gate. A separate cached equal-rotation-average test improves clean scores 4.02/10.94/5.05% on 0021/0027/0060 (14/16 sections; two regressions retained). [subset-ensemble-v1](experiments/subset-ensemble-v1/results.md) therefore tests averaging, not a confidence gate: previous gap2 optics + accepted edge fix, original calibration/rebase/decay/handback, full-fit fallback if either subset lacks support. New 0073 MP4 is verified; all six rebase decisions unchanged; clean-control rate delta 0.00338 deg/s. Full render and four excerpts are complete and validated (37594 full frames; 1600/1400/300/800 excerpt frames). Review `target/experiments/subset-ensemble-v1/compare_{late,early,control,additional}.mp4`: LEFT previous gap2-edge, RIGHT subset average. Late priority excerpt 10.5–11.75 s. User verdict on late: possible tiny improvement, judder essentially identical; other windows unjudged. No production promotion or shipping rebuild; no process needs resuming.

**Latest uncapped tracking diagnostic:** [tracking-diagnostics-v1](experiments/tracking-diagnostics-v1/results.md) acquired 9981 unchanged-tracker pairs across the 16 clean sections and 0073 late/control windows. Rates and quality match saved caches to floating-point precision; full requested cadence/counts validated. Late gap2 median support is 558 inliers across 11/12 cells. Uncapped counts, LK error, in-sample Sampson distance and spatial coverage do not consistently predict the remaining gap2 clean-reference error, including cross-clip thresholds and time controls. No new repair/render or gate. Next test independent spatially balanced feature-subset rotation stability using the same estimator, full-fit parity and clean/synthetic controls; do not change lens/scaling or expand the geometric model.

**Latest read-only optical reliability study:** [optical-reliability-v1](experiments/optical-reliability-v1/results.md) reuses all 16 clean sections / 5622 scored pairs, reproducing existing frozen held-out scores. Current quality saturates at 210 essential inliers: all scored 0060 and all 0073 late one-frame samples have quality 1. Span disagreement has only weak association with remaining gap2 error (within-run correlations 0.055/0.118/0.020); time-shift controls and cross-clip threshold transfer do not support a reliable gate. No new repair/render. Next acquire uncapped tracking diagnostics (inlier counts, residuals, LK errors, spatial support) in a read-only copy of the unchanged tracker, verify rate/quality parity, then test reliability on the same clean references before proposing any correction. Lens/scaling and estimator expansion remain deferred.

Reduce the remaining visible small, sharp judders and unwanted panning in DJI O4 Pro footage. The user repeatedly finds marginal numerical improvements hard to distinguish visually and wants a clearly visible benefit. They are willing to inspect clips when a concrete comparison is ready. Their latest instruction is to defer lens correction and pixel-scaling investigations and prioritize higher-confidence gyro-related causes of stabilization judder.

Production remains the reviewed M2 default with optional M4. No research candidate has been promoted. The strongest demonstrated fix is **rebase-offset-induced motion in splice edge blends**, following the visually successful shorter ramp. Active research now concerns the smaller residual jello/vibration after that fix. See [exact trace and edge-offset experiments](experiments/gyro-trace-v1/results.md). The [splice exposure audit](experiments/splice-exposure-v1/results.md) finds detected noisy samples inside raw-orientation edge ramps at all four complaint windows. The user now confirms that the 0.19 s ramp candidate is visibly less wobbly, including at 308 s despite the higher sharp-shake proxy. That first improvement is now superseded by the **fixed-edge-offset candidate with 0.19 s ramps and original 0.20 s padding**: the user reports significant improvement, nearly unnoticeable judder at 106/225/308 and only a tiny residual vibration at 249. This is the preferred research baseline; additional 0071/0073 visual validation now confirms an overall improvement, at worst no worse and at best a complete fix. Mild horizontal bouncing remains at 0073 ~135 s and occasional slight panning during full-throttle forward flight. Production is not yet promoted. Lens/scaling and further geometric estimator work are deferred at the user's request; their existing results remain preserved.


## Reading order

1. This handoff.
2. [Code review and production safeguards](code-review-2026-09-09.md).
3. [Remaining task/experiment backlog](superpowers/plans/2026-09-09-stabilization-follow-up-backlog.md).
4. The recent edge-offset, generalization, residual-stage and bounce-localize reports; spatial/lens reports are historical deferred work.
5. Historical `CLAUDE.md`, `docs/superpowers/`, and `.claude/` notes for prior dead ends and implementation context. New results and user verdicts supersede old status, not the historical measurements themselves.

## Experiment ledger and human verdicts

| Experiment | Numerical result / purpose | User verdict and disposition |
|---|---|---|
| [Contiguous-run calibration](experiments/calibration-v1/results.md) | Chronological runs, real cadence, joint shift/mapping refit. Pooled held-out error changed +0.036% on 0021, -0.530% on 0027, +0.180% on 0060. Synthetic cases passed. | No render warranted; mixed/negligible benefit. Keep experimental fitter out of production. |
| [Residual localization / zero-decay](experiments/residual-v1/results.md) | No low-confidence pairs at the four complaint windows. All overlap monster-noise segments. Setting decay 0 removes the slow return of accumulated rebase offset. | “Extremely similar,” both still judder. No meaningful visual improvement. Retain decay 1.5 deg/s. |
| [Forward/backward tracking gate](experiments/tracking-v1/results.md) | 1 half-resolution pixel round-trip gate worsened 0060 common-support held-out error 3.294→3.414 deg/s (+3.65%). | No promotion/render. |
| [RANSAC seed control](experiments/tracking-v1/results.md) | Seed +1 caused no measurable change. No-gate research tracker matched cached production to 8.9e-16 rad/s. | A cached-JSON exact assertion failed only at floating-point round-off; replaced by 1e-12 tolerance, not a relaxed physical-error criterion. |
| [Two-frame optical measurement](experiments/tracking-v1/results.md) | Same 100 Hz output, pairs span 20 ms. Clean held-out error fell 24.9% / 40.2% / 35.5% on 0021 / 0027 / 0060; all 16 sections improved. Candidate retained one-frame calibration and used two-frame noisy tracking. | Maybe slightly less judder, but would be hard to identify without labels. Weak/uncertain visual benefit, not a clear improvement. No default change. |
| [Localized post-render smoothing](experiments/local-smooth-v1/results.md) | Similarity-based output correction, Gaussian sigma 40/80 ms, bounded 22 original pixels / 0.5°, common 1.08 zoom. Lower 2–8 Hz roll, higher 8–30 Hz translation and roll in every window. | User confirms wobble becomes sharper shake and looks worse. **Retire both variants.** This was a post-render research stage, not gyro repair. |
| [Spatial held-out model comparison](experiments/spatial-v1/results.md) | Homography predicts held-out tracks better than similarity in all four windows; split-fit stability improves strongly at 106/225 s, moderately at 308 s, not at 249 s. Upper-right sky region at 249 s remains discrepant. | Read-only diagnostic, not a validated replacement stabilizer. Supports model mismatch; does not prove rolling shutter or parallax as sole cause. |
| [Rolling-shutter off](experiments/rs-v1/results.md) | Same default repaired video; readout compensation 0 vs 5.092569 ms, otherwise matching Gyroflow settings. Real output difference verified. | Right/off possibly slightly better, but close, no drastic improvement. Keep current setting; deprioritize as principal cause without claiming it is ruled out. |

## Strongest current evidence and limits

Similarity vs homography median held-out pixel errors at half resolution:

| Moment | Similarity | Homography | Split disagreement, similarity → homography |
|---|---:|---:|---:|
| 106 s | 0.830 | 0.226 | 0.387 → 0.049 |
| 225 s | 0.713 | 0.253 | 0.256 → 0.086 |
| 249 s | 0.653 | 0.392 | 0.182 → 0.191 |
| 308 s | 0.592 | 0.329 | 0.217 → 0.153 |

At 249 s, one region has ~3.06 half-resolution pixels of residual while most are 0.3–0.7. An inspected frame shows sky there. Depth-dependent parallax, ambiguous cloud tracks and deformation remain possible. A homography can fit a dominant plane while failing at other depths. Rendered image rows are not sensor readout rows. Neither held-out fit accuracy nor a lower optical proxy is proof of better perceived stabilization.

Approximate reconstruction of default handback is zero at 105.83–106.61, 223.44–223.96 and 225.03–226.18 s. It averages 5.7% at 248.34–249.32 s (brief peak 85%), and 0.75% at 308.01–309.28 s. Different cached window boundaries mean this is not exact production instrumentation. Handback is a weak common explanation for these particular moments, but not ruled out in other fast motion.

## Next bounded research step

The partner-only ablation lacks meaningful late-clip benefit. Optical-reliability-v1, tracking-diagnostics-v1 and subset-stability-v1 do not establish a reliable confidence gate. The completed subset-average trial now has a late-clip verdict: possible tiny improvement, essentially unchanged judder. Keep the accepted baseline and prior gap2 candidate. Reassess shared systematic errors and what the observable residual represents before another estimator/gate sweep; do not infer a physical cause or reopen deferred lens/scaling/geometric expansion. These decisions supersede older acquisition, pending-review and parameter-trial next steps below.

1. Preserve the visually accepted `target/experiments/gyro-trace-v1/edgeoffset.MP4` and `edgeoffset-render.mp4` as the preferred research baseline. The user confirms a significant improvement in all reviewed clips; at 106/225/308 judder is almost unnoticeable. Only a tiny vibration remains at 249 s. The separate wider-padding candidate has no user verdict and is not part of this accepted combination.
2. The extra-rebase / endpoint-bridge ablation is now visually negative: the user sees little improvement in pan/bounce. Retain original 30 deg/s rebase gate and 1.5 deg/s decay with the accepted edgeoffset + 0.19 s ramps; do not adopt zero-trust forced rebasing. Stage rate differences are not proof of perceptual causation. Localization is now complete: no cadence gaps/exact source duplicates in 132–146 s, and several apparent peaks include intended motion. Optical estimates show material 10 ms versus 20 ms span sensitivity, including inside an already rebased burst. See [bounce-localize-v1](experiments/bounce-localize-v1/results.md). A controlled candidate combines the earlier two-frame optical measurement with the accepted edge fix (earlier two-frame visual benefit preceded the edge fix and was weak). The user has reviewed it: a small improvement, with subtle jello/vibration still present in both clips. No separate clean-control verdict. Preserve this incremental candidate and the stronger cross-clip baseline. Next correlate residual events with remaining contaminated raw-gyro contribution versus fully optical interiors using existing traces; do not infer accuracy from span differences alone. Lens/scaling stays deferred.

3. Preserve the smaller remaining 0060 249 s vibration as another target. Additional frozen-candidate validation on 0071/0073 is complete with positive user verdict: mostly better, sometimes subtler, at worst no worse and at best a complete fix. Residuals mean this is not universal resolution. Production integration has not been requested or performed; it would require appropriate regressions and rebuilding both shipping binaries. Keep lens/scaling deferred and preserve the separate unreviewed pad040 experiment.


The edge-offset change eliminates a demonstrated synthetic artificial-motion mechanism, preserves known multi-axis motion and non-rebased behavior, and now has a positive user visual verdict. Do not let mixed image-motion proxies override that verdict. Historical pending-review and earlier optical/lens next-step notes below are superseded by this section.

## Clips, windows, and artifact locations

Workspace: `C:/Users/lzhan/Desktop/o4prostab`.

- Source of main complaints: `sample_vids/DJI_20260808151831_0060_D.MP4`.
- Preserved default repair: `sample_vids/DJI_20260808151831_0060_D_fixed.MP4`. Never overwrite either.
- Agreed moments: 1:46 / 3:45 / 4:09 / 5:08 (106 / 225 / 249 / 308 seconds).
- Review windows: 102–110, 221–229, 245–253, 304–312 s.
- Verified clean visual control: 19–22 s, inside an actual calibration section.
- **117–128 s is not clean for 0060**: it contains two severe bursts and a 0.20 s low-confidence interval. Older clean labels belonged to 0021. Do not reuse them across clips.
- Development/calibration clips: 0021, 0027, 0060. Newly supplied 0071 and 0073 are additional frozen-candidate validation clips, not tuning targets for the first pass. Keep 0057–0059 out of tuning.
- Compact durable reports, numerical scores and fingerprints: `docs/experiments/{calibration-v1,residual-v1,tracking-v1,local-smooth-v1,spatial-v1,rs-v1}/`.
- Large local raw observations, MP4s, logs and context AVI files: `target/experiments/`. These are ignored/build artifacts, not guaranteed to survive cleanup or transfer. The reports preserve conclusions and reproducibility details; filenames/hashes are in manifests.
- Preserved baseline binaries: `target/experiments/calibration-v1/baseline/`.
- Validated default full render reused in comparisons: `target/experiments/decay-v1/baseline.mp4`.
- Candidate full renders: `decay-v1/hold.mp4`, `tracking-v1/gap2.mp4`, `rs-v1/rs_off.mp4` under that same target experiments root.
- Two-panel exports: `compare_106.mp4`, `compare_225.mp4`, `compare_249.mp4`, `compare_308.mp4` (and controls) under the relevant experiment directory. Left default, right candidate. Local-smooth-v1 has three panels: baseline / conservative / strong, and extra 1.08 zoom on all panels.

## Production invariants and known pitfalls

- M2 default, optional M4; embedded-quaternion MP4 workflow. No numerical research winner has become a shipping default.
- Segment-PEAK noise trust ramp 200–300 deg/s. Do not substitute instantaneous noise gating or ungated min(gyro,optical); these were measured dead ends.
- Rebase threshold 30 deg/s uses 1.5*drift/duration; decay 1.5 deg/s. Constant world-frame offsets are invisible to relative stabilization with horizon lock off; decaying offsets are not mathematically identical.
- Preserve severe/optical segment coverage checks and transactional verified output. Do not silently rebase filtered-gyro fallback.
- Keep horizon lock off for these comparisons. No autosync. Empty offsets. Remove stale gyro_source.file_metadata.
- Gyroflow projects must use actual 0060 metadata: 1440x1080, 100 fps, 38269 frames, 382690 ms (duration_ms, not duration). Output 1440x810. Full-clip rendering preserves smoothing context; extract short excerpts afterward.
- Compare Gyroflow corrections in body coordinates (org inverse * stabilized), not conjugated frame-domain values. A source-to-fixed quaternion difference is not the same quantity.
- Do not revisit global temporal gyro low-pass, rate-weighted drift spreading, old Gyroflow glitch filtering, gcsv delivery, or autosync without new evidence. The old task-5 negative rebase report used superseded zoom-corrupted renders.
- Python remains a historical reference with missing failure-path safeguards; don't use it to deliver experimental repairs.
- Research copies under examples/support intentionally isolate experiments. They are not ready to be duplicated into production; refactor a shared path and rerun parity if a candidate is selected.

## Code and verification state

Earlier review changes are still in the working tree: transactional MP4 writes and alias checks, configuration/telemetry/calibration validation, accepted optical coverage, GUI queue/start/cancel bookkeeping safeguards, and evaluation-label fixes. See the code-review report for details and remaining parser/filename/cancellation/product priorities. Preserve those changes and pre-existing `.claude/`, `.vscode/`, `test_gyro.csv`; do not reset the workspace.

Last full validation after calibration experiment: **33 regular tests + 14 private regressions passed**, including full M2/M4 fixtures and exact sign-folded 0060 quaternion parity with five rebases. Both shipping release binaries were rebuilt then. Subsequent research touched examples/docs only, not production core defaults. Later targeted tests: 2 localized-smoothing math/crop tests and 2 spatial-model synthetic tests passed. Research repair variants passed optical coverage and timestamp/quaternion writeback checks; full renders and excerpt frame counts were validated. These counts refer to their actual runs, not a freshly repeated suite after every documentation change.

Important research code:

- `o4core/src/alignment.rs`, `o4core/tests/calibration_experiment.rs`: experimental calibration, not default pipeline.
- `o4core/examples/calibration_probe.rs`, `residual_probe.rs`: initial acquisition/diagnostics.
- `tracking_probe.rs`, `tracking_seed_probe.rs`, `tracking_gap_probe.rs`, `tracking_score.rs`, `tracking_gap_repair.rs`, `support/fb_tracker.rs`, `support/gap_patch.rs`: tracking experiments and isolated verified repair.
- `local_render_smooth.rs`, `local_render_measure.rs`, `handback_probe.rs`: retired output smoothing, exported-panel measurements, approximate handback diagnosis.
- `spatial_motion_probe.rs`: current read-only persistent-track / held-out spatial diagnostic; start here for the next estimator investigation.

Changes are **not committed**. New documents/examples are currently untracked and therefore persist in this workspace but are not automatically present in a fresh checkout/worktree. A new session should use this same directory or explicitly transfer/commit the working tree first. No commit, branch, merge or publication has been requested.

## Runtime / reproduction notes

Windows PowerShell, Rust 1.88. Environment for Cargo/OpenCV commands:

```powershell
$env:OPENCV_INCLUDE_PATHS='C:\opencv\build\include'
$env:OPENCV_LINK_PATHS='C:\opencv\build\x64\vc16\lib'
$env:OPENCV_LINK_LIBS='opencv_world4120'
$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'
$env:PATH += ';C:\opencv\build\x64\vc16\bin;C:\Program Files\LLVM\bin'
cargo build --release --offline -p o4core --examples
```

Each example prints positional-argument usage if called without arguments. Tests: `cargo test --offline --workspace`; private checks: `cargo test --release --offline -p o4core -- --ignored` (includes expensive full optical runs). After any production core change, rebuild **both** shipping binaries with `cargo build --release --offline --workspace`; the user runs target/release CLI/GUI, and stale GUI builds caused prior confusion.

- FFmpeg / ffprobe: `C:/ffmpeg/bin/ffmpeg.exe`, `ffprobe.exe`.
- Portable Gyroflow: `tools/gyroflow-portable/Gyroflow.exe` v1.6.3. Sandbox access required escalation and was approved previously; don't use the Store/MSIX app for CLI arguments. Launch background renders with Start-Process -WindowStyle Hidden -PassThru and separate stdout/stderr logs. The GUI-subsystem process can return before rendering finishes; confirm output completion and ffprobe metadata.
- OpenCV failed to seek the long HEVC render at 220 s even though FFmpeg decoded it. Later diagnostics used lossless FFV1 BGR contexts at source offsets 220, 244, 303 s, saved under local-smooth-v1. Preserve time offsets when evaluating gates.
- Default Python is miniconda 3.12 without numpy/scipy/cv2; use Rust/OpenCV and stdlib Python. Bundled Python has numpy but not scipy/cv2/matplotlib. Avoid assuming Python reference scripts run here.
- Git may need `git -c safe.directory=C:/Users/lzhan/Desktop/o4prostab ...`; don't alter global Git config just for this. Some user gitignore/.pytest_cache permissions cause harmless read warnings.
- Existing nom 6.1.2 future-compatibility warning remains.

## Resumed investigation - persistent perspective validation

See [perspective-v1 results](experiments/perspective-v1/results.md). Added isolated `o4core/examples/perspective_track_probe.rs` with survivor-preserving replenishment, mature-track comparison and whole-region-held-out validation. All four complaint windows plus 19-22 s control completed; three synthetic tests passed. No production changes, new correction videos or shipping rebuilds.

At 249 s mature-track aggregate error improves but bottom-left withheld-region error reaches 4.415 half-resolution pixels; sky support drops to 43/98 pairs. At 308 s mature bottom-right error reaches 3.436. Do not interpret track-age selection as a validated fix. Next add conditioning/spatial support and common-support regional comparison, then synthetic camera rotation/fast turns/clustered points/multiple planes before any correction. The report records limits and next steps.

## Resumed investigation - conditioning and common support

[perspective-v2](experiments/perspective-v2/results.md) completes the geometry/synthetic next steps. Added isolated `perspective_condition_probe.rs`; eight synthetic tests and all five video contexts passed. On identical withheld features, mature selection improves 249 sky but worsens middle-left/bottom-left/bottom-middle and 308 bottom-right. No projective poles or rank deficiencies occur in the evaluated common-support intervals, but sensitivity and split disagreement remain material. Two-depth synthetic motion demonstrates that a numerically stable homography can still fail a different region.

Do not promote mature-track selection or proceed to full projective smoothing. Next: calibrated **source-image** bearing/rotation versus translation-aware pose estimation using actual lens metadata, independent regions, clean held-out telemetry and synthetic motion/degeneracy checks. See the v2 report for limits and reproduction. Production and shipping binaries remain unchanged. Prior perspective-v1 artifact fingerprints are preserved; the new experiment has its own files.

## Resumed investigation - calibrated source pose

[source-pose-v1](experiments/source-pose-v1/results.md) completed 16 clean intervals across 0021/0027/0060 (6195 source pairs). Added isolated `source_pose_probe.rs` and saved normalized persistent correspondences. Fixed a stationary-case round-off NaN in the research angle calculation. Five synthetic tests and Clippy pass.

Rotation-only loses held-out telemetry consistency (+41% on limited 0021 coverage, +171%/+167% on 0027/0060). Persistent essential acquisition is mixed, worse on 0027/0060. Positive-depth recovery selects the same rotation; relaxing its distance filter restores coverage without a different rotation. No candidate proceeds to repair/render or production. Next: a bounded joint multi-frame rotation/translation/depth experiment with independent tracks and synthetic observability checks, explicitly distinct from the already-tested wider pair gap. See report for data coverage and limitations. Production/release binaries unchanged.

## Resumed investigation - first joint multi-frame prototype

[multiframe-v1](experiments/multiframe-v1/results.md) implements independent six-frame poses and shared inverse depths without a temporal smoothing prior. Added isolated `multiframe_pose_probe.rs` and `multiframe_score.rs`. Seven synthetic tests and Clippy pass. The initial solver was repaired to use positive log depths and reject individually infeasible initial projections; both initial and final artifacts are retained.

On the clean 0060 control, 390/399 pairs have finite iterates and 339 converge. Finite-iterate RMS is 4.278 versus production 2.903 deg/s on 313 common scorer samples. Fully converged coverage is fragmented (longest run 58 pairs), so validated common-support RMS remains null. No repair/render or production change is justified.

Next: investigate the fixed-first-bearing noise assumption by allowing landmark bearings to refine, inspect unregularized pose observability, and check initialization sensitivity on this same clean control before broadening. This is unfinished solver research, not proof that multi-frame geometry cannot help. Shipping binaries and prior working-tree changes remain unchanged; new example binaries are rebuilt.

## Resumed investigation - refined landmarks and observability

[landmark-v1](experiments/landmark-v1/results.md) adds full landmark-bearing refinement and unregularized, scale-gauge-projected pose diagnostics in isolated `landmark_bundle_probe.rs`. Nine synthetic tests and Clippy pass. Both default and perturbed-initialization runs completed on the same 399-pair clean control.

Finite-iterate RMS worsens to 7.569/6.978 versus production 2.903 deg/s on 313 common samples. Fully converged coverage still cannot support the established scorer. Most windows have generic local rank, yet some converged solutions differ by up to 6.836 deg/s after a small initial perturbation. No correction/render or production change.

Next: stop expanding this rigid-model solver for now. Inspect held-out source residuals versus actual source row/radius and angular speed, across multiple clean sections with negative controls, before proposing a targeted physical-model change. Correlation alone will not establish rolling shutter/parallax/lens error; prior RS-off visual verdict remains inconclusive. See report for constraints and reproduction. Earlier code/artifacts and shipping binaries remain unchanged.

## Resumed investigation - lens inverse and source residual patterns

[lens-v1](experiments/lens-v1/results.md) found an upstream inverse-branch problem in the supplied fisheye polynomial. On a 4015-point source grid, default OpenCV inversion fails a 0.1px round trip at 195 locations; 51 additional locations round-trip through a different branch. Principal-branch bisection passes the tested full-image round trips. However, the isolated corrected-inverse tracker changes held-out RMS -4.45% / -0.05% / +11.01% on 0021 / 0027 / 0060, so it is not promoted. Production unchanged.

Rebuilt all 6195 persistent source pairs with original pixel coordinates retained under target/experiments/lens-v1/*-source.json. These explicitly identify principal inversion. Earlier normalized-only caches are preserved but cannot independently verify source locations after failed/alternate inversions; do not interpret old geometry failures as blanket rejection of clean geometry.

The verified held-out epipolar study finds increasing error magnitude with radius in all 16 sections, but no consistent signed row/radial or speed pattern identifying rolling shutter. Negative spatial/time controls and limitations are in the report. All runs completed, 13 test executions (shared helper tests included) and Clippy pass. No repair/render or shipping rebuild.

Next: source-pixel-aware geometric error/uncertainty using the lens Jacobians and both image observations, first tested against synthetic pixel noise and then frozen-calibration clean comparisons on these verified caches. Do not tune readout time from correlation or promote the inverse solely for mathematical round-trip consistency.


## Latest review ready: fixed edge offsets

See [gyro-trace-v1 results](experiments/gyro-trace-v1/results.md). Full renders and all excerpts validated; edgeoffset comparisons are the first visual priority (left preferred ramp019, right fixed edge offsets). Apparent 2–8 Hz roll changes -81/-25/-6/-57% at 106/225/249/308, with mixed high-band changes. Do not treat this as a perceptual verdict. Wider padding is a separate secondary candidate. Latest verdict: edgeoffset is a significant visual improvement and the preferred research baseline; only pad040 remains unreviewed. Production unchanged.


## Additional scenario validation: 0071 and 0073

User added `DJI_20260829140442_0071_D.MP4` (14–17 s throttle pin; 255–257 s unknown trigger; user corrected the initial 74–77 s annotation and confirmed that interval clean) and `DJI_20260829141435_0073_D.MP4` (17–27 s powerloop then severe high-throttle judder; 135–140 s additional shaking). See [generalization-v1](experiments/generalization-v1/results.md).

Frozen candidate hash verified. Both clips passed coverage and baseline/candidate writeback, unchanged drift/rebase decisions. Baseline comparisons use production 0.30 s versus candidate fixed edge offsets + 0.19 s, keeping 0.20 s padding. No clip-specific tuning. 0071 has 1 rebase, 0073 has 6. Corrected 0071 event 14–17 s matches detection 14.168–17.103 s; review 11–21 s. The original apparent mismatch is not a detector failure. Controls are user-confirmed clean 0071 74–77 s and telemetry-clean 0073 55.9–58.9 s. All four full renders and six excerpts completed and validated; decoded layouts inspected. Compare paths: `generalization-v1/0071/compare_{throttle,late,control}.mp4` and `0073/compare_{early,late,control}.mp4`. Left production, right frozen accepted candidate. First 0071 event is severe (peak455.6) but not rebased (23.77 degree drift over2.935s), a distinct endpoint-bridge test. User verdict: overall improvement, at worst no worse and at best a complete fix; residual 0073 ~135 s horizontal bouncing and occasional throttle-pin panning remain. Production unchanged.


## Residual-stage ablation — no meaningful visual improvement

See [residual-stage-v1](experiments/residual-stage-v1/results.md). Exact accepted-repair parity passes on 0071/0073. Endpoint bridging adds 9.92°/s RMS in the 0071 throttle interior and 7.51/3.24°/s in selected 0073 interiors. Handback is comparatively small there. A research ablation additionally rebases severe intervals wholly inside existing zero-trust optical segments, retaining all other accepted settings and decay. Both new MP4s verified; adds 2 rebases to 0071 and 4 to 0073. A synthetic bridge-removal / unselected-parity test and Clippy pass. Extra offsets change later decay: 0073's clean control has ~3°/s slow rate difference, so watch for displaced panning. Large constant world-angle differences are not direct stabilization error. Render was initially blocked by usage-limit auto-review; resumed after user continued. New comparison left=accepted edgeoffset, right=trustrebase. No production promotion; subsequent user verdict was no meaningful pan/bounce improvement (see below).


Residual-stage renders and seven excerpts are now complete and validated, with decoded layouts inspected. Review `target/experiments/residual-stage-v1/0071/compare_throttle.mp4` and `0073/compare_{early,late,control,extra_rebases}.mp4`; left is accepted edgeoffset, right is added zero-trust rebasing. The 0073 clean control is important because extra rebases change slow offset-decay motion. User verdict: little/no meaningful pan-bounce improvement; retain accepted edgeoffset without added rebases. No separate control verdict supplied. Production unchanged.


Latest decision: zero-trust forced rebasing did not materially improve the remaining pan/bounce. Preserve it as a negative experiment; the preferred candidate remains fixed edge offsets + 0.19 s ramps with original rebase/decay settings.


## Remaining bounce: localization and combined optical-span trial

[bounce-localize-v1](experiments/bounce-localize-v1/results.md) separates intended camera motion from apparent output motion without subtracting incompatible units. Frame cadence is regular and no exact source duplicates occur in 0073 132–146 s. Optical span sensitivity warrants a narrowly controlled revisit of gap2 plus accepted edge offsets. New gap_edge_repair passed coverage and MP4 writeback, original six rebase decisions unchanged; clean-control rate difference 0.114 deg/s. Four new examples pass Clippy. Full render and all three comparisons completed and validated (37594 full frames, 1600/1400/300 excerpt frames at 100 fps); decoded layout inspected. User verdict: small improvement, residual subtle jello/vibration remains in both clips; no separate control verdict. Candidate target/experiments/bounce-localize-v1/gap2edge.MP4, comparisons compare_{early,late,control}.mp4 (left accepted edgeoffset, right gap2edge). Production and shipping binaries unchanged.


## Latest experiment inventory and exact restart commands

All paths below are relative to `C:/Users/lzhan/Desktop/o4prostab`; use this same workspace because much work is untracked.

| Item | Location / purpose |
|---|---|
| Accepted 0060 repair/render | `target/experiments/gyro-trace-v1/edgeoffset.MP4`, `edgeoffset-render.mp4` |
| Accepted 0071/0073 repair/render | `target/experiments/generalization-v1/{0071,0073}/edgeoffset.MP4`, `edgeoffset-render.mp4` |
| Latest combined candidate | `target/experiments/bounce-localize-v1/gap2edge.MP4`, `gap2edge.gyroflow`, `gap2edge-render.mp4` (0073) |
| Latest user-reviewed comparisons | same directory: `compare_early.mp4` (14–30 s), `compare_late.mp4` (132–146 s), `compare_control.mp4` (55.9–58.9 s); LEFT accepted edgeoffset, RIGHT gap2edge |
| Exact optical/splice traces | `target/experiments/residual-stage-v1/{0071,0073}/trace.json`, `stages.json`; cached accepted parity passed |
| Intended path / localization | `target/experiments/bounce-localize-v1/camera.json`, `localization.json`, `bins.json` |
| Optical span observations | same directory: `spans.json`, `span-scores.json` |
| Verification | same directory: `repair.log`, `render.log`, `event-rates.json`, `review-validation.json`, `review-frame.png` |
| Durable small reports | `docs/experiments/bounce-localize-v1/`: results, manifest hashes, candidate bursts, cadence, bins, span scores, event rates, review validation, export/summarize scripts |

New code: `bounce_localize.rs` (read-only right-panel motion and intended path, separate units), `optical_span_probe.rs` (10/20 ms optical acquisition), `optical_span_score.rs` (LP8 common-midpoint sensitivity), `gap_edge_repair.rs` (existing gap_patch + edge_offset_patch). Imported `local_render_smooth.rs` supplies the tracker only; no output smoothing is applied. All four new examples passed release offline Clippy with warnings denied. MP4 coverage/writeback and full/excerpt frame counts passed. No new full production test suite was needed/run for these examples-only changes.

After setting the OpenCV environment above, commands are:

```powershell
# Read-only acquisition/scoring (outputs already exist; do not rerun by default):
target/release/examples/optical_span_probe.exe sample_vids/DJI_20260829141435_0073_D.MP4 target/experiments/bounce-localize-v1/spans.json
target/release/examples/optical_span_score.exe target/experiments/bounce-localize-v1/spans.json target/experiments/bounce-localize-v1/span-scores.json
target/release/examples/bounce_localize.exe target/experiments/generalization-v1/0073/compare_late.mp4 132 target/experiments/bounce-localize-v1/camera.json target/experiments/residual-stage-v1/0073/stages.json target/experiments/bounce-localize-v1/localization.json
python docs/experiments/bounce-localize-v1/summarize.py
# Repair only to a NEW destination; do not overwrite preserved candidate:
# target/release/examples/gap_edge_repair.exe SOURCE NEW_DESTINATION
```

Gyroflow `--export-metadata 3:ABSOLUTE_PATH` exports camera data **instead of rendering**, using the supplied .gyroflow project. Exported camera JSON is a frame list with timestamp_ms, org_quat/stab_quat (wxyz), Euler angles and FOV data. The 0073 export has 37594 frames; timestamps begin at half-readout (~2.546 ms). Full render is launched separately. Portable Gyroflow may require approved sandbox escalation; never bypass an auto-review rejection. Use hidden Start-Process and distinct logs, then confirm completion with ffprobe. Completed latest render took about 199 seconds.

Additional interpretation limits for the next session: source and accepted render have regular 10 ms cadence and no adjacent exact source duplicates in 132–146 s, which does not rule out a global sync error. At 143.50–143.75 s an apparent horizontal peak occurs well inside a rebased interval while intended path motion is small; this is suggestive, not proof of rate-estimation error. Gap1/gap2 LP8 disagreement is as high as 12.13 deg/s in a quarter-second bin, but disagreement alone is not accuracy. Latest candidate keeps all original six rebase decisions, with clean-control rate difference 0.114 deg/s. User perception remains the deciding evidence.
