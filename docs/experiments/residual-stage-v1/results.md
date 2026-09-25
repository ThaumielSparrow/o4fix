# Residual attribution and zero-trust endpoint-bridge ablation

Research only. Baseline is the visually accepted fixed-edge-offset repair, 0.19 s ramps and original 0.20 s padding. Production is unchanged.

## Exact attribution

`residual_stage_probe` reruns the instrumented production optical patch, reconstructs the accepted repair and verifies parity against its preserved MP4. Maximum sign-folded quaternion component discrepancies are 3.9336e-8 for 0071 and 3.9984e-8 for 0073 (stored precision). It measures final body rates minus supplied patch rates separately inside the fully replaced interior and at the edges. Optical is not ground truth.

Selected non-rebased intervals:

| Clip / interval | Endpoint drift | Implied bridge peak | Interior rate difference RMS |
|---|---:|---:|---:|
| 0071 14.168–17.103 s | 23.77° | 12.15°/s | 9.92°/s |
| 0071 18.730–19.230 s | 3.66° | 11.01°/s | 10.81°/s |
| 0073 19.492–22.581 s | 13.90° | 6.75°/s | 7.51°/s |
| 0073 134.714–135.394 s | 1.55° | 3.42°/s | 3.24°/s |

The bridge-peak formula is 1.5*drift/duration. It is not an upper bound on the total body-rate difference: the body-frame quaternion correction also changes the frame of the integrated rates. Rebased interiors match supplied rates to numerical precision, as expected. Remaining motion in those interiors is already in the patch, not newly introduced there by the splice.

Exact handback is small in these selected non-rebased intervals: RMS added rates are 0.0052/0/0.3034/0.0294°/s respectively. In the 0073 rebased 135.766–136.628 s interval it is 0.156°/s RMS (0.701 peak). This narrows attribution; it does not prove that handback or optical estimates are error-free. Complete selected summaries are in the per-clip folders; full trace data remain in target.

## Isolated experiment

`trust_rebase_repair` adds a rebase when a severe interval is wholly contained in an optical segment whose existing segment-peak trust is exactly zero (peak noise at least 300°/s). It retains the original drift-based rebases. This tests whether bridging back to an endpoint in a segment already distrusted for rate estimation contributes to the remaining pan/bounce. It does not establish that endpoint error and rate-noise trust are equivalent.

No new numeric threshold, tracking change, temporal smoothing, padding change or handback change is introduced. The same cached production rates, accepted fixed edge offsets, 0.19 s ramps and 1.5°/s offset decay are retained. Optical coverage checks precede the splice. Input-cache parity against the accepted MP4 is checked before generating each new repair.

New rebases: 0071 at 14.168–17.103 and 18.730–19.230 s (total 1→3); 0073 at 19.492–22.581, 134.714–135.394, 267.060–267.981 and 268.434–269.466 s (total 6→10). Selection is applied throughout each clip, not only at user-marked events. Both candidate MP4s passed timestamp and sign-folded quaternion writeback verification.

A stationary synthetic with perfect zero rates and a 24° drift over a 3 s interval produces a 12°/s bridge under the accepted baseline and zero in-burst rate under the selected rebase. With no selected interval, the research helper is bit-identical to the accepted splice. This test passes; targeted Clippy passes. It demonstrates mechanism removal, not correctness of optical rates on real footage.

## Important control

Extra rebases carry additional offsets forward and change their later decay. In 0073's 55.9–58.9 s clean control, the candidate differs from the accepted baseline by approximately 3°/s RMS, predominantly slow motion; 2–8 and 8–30 Hz magnitudes are almost unchanged. The large world-orientation difference (~104° there) is not itself a stabilization error with horizon lock off, but the changed decay rate direction can affect output. The controls must be reviewed; removing a burst bridge is not sufficient evidence of an overall improvement. 0071's confirmed 74–77 s control remains unchanged to round-off.

## Resume and reproduction

The usage limit interrupted automatic approval review before either renderer started. Traces, stage metrics, verified candidate MP4s and projects had already completed. After the user authorized continuing, both identical render calls were accepted and resumed; no optical analysis rerun was needed.

Large files: `target/experiments/residual-stage-v1/{0071,0073}`. `trace.json` holds rates and source weights; `stages.json` holds attribution; `candidate.json` records new rebase decisions. Candidate repair is `trustrebase.MP4`, full render is `trustrebase-render.mp4`. `export_reviews.py CLIP` exports the same corrected event windows and controls after validating full render metadata. Left is accepted edgeoffset, right is the bridge ablation. User verdict: no meaningful pan/bounce improvement; no promotion.


## Render and review completion

Both full renders completed after resuming. Full metadata checks passed (1440x810, 100 fps, source-specific 38269/37594 frames). Seven comparison excerpts passed frame-count verification: 0071 throttle/late/control and 0073 early/late/control/extra_rebases. The extra 0073 review spans 264–272 s to cover the two additional unmarked bursts selected by the clip-wide zero-trust rule. Decoded event frames from both clips were inspected for correct two-panel layout.

Left = accepted fixed-edge-offset candidate; right = additional zero-trust rebasing. The original production default is not the left panel in this experiment. Review 0071 throttle, 0073 early and 0073 late for remaining pan/bounce, then 0073 control and extra_rebases for displaced slow motion/regressions. No visual improvement is claimed from telemetry differences alone. User verdict: no meaningful pan/bounce improvement. Production unchanged.


## User verdict — no meaningful improvement

User reports they do not see much improvement in the pan/bounce. Classify this zero-trust extra-rebase ablation as no meaningful demonstrated visual benefit. Do not infer a separate verdict on the clean control or claim the candidate was worse; neither was stated. Retain the accepted fixed-edge-offset candidate with 0.19 s ramps, original 0.20 s padding, original 30 deg/s rebase gate and 1.5 deg/s decay. Do not promote the added zero-trust rebase rule or repeat this broad bridge-removal experiment without new evidence.

The stage measurements establish a contribution to stored orientation rates, not the cause of the remaining visible motion. Next diagnostic priority: localize the residual in the accepted rendered 0073 132–146 s window against exact splice phases and optical measurements, separating intended camera motion from residual bounce. In already rebased interiors, accepted output matches supplied rates to numerical precision; investigate the measurements and frame timing there before more rebase/decay sweeps. Prior two-frame optical results were only weakly positive on 0060; any follow-up must address this residual specifically. Lens/scaling remains deferred.
