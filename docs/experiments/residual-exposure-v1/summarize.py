"""Join exact telemetry exposure with existing image-motion bins. Run at repo root."""
import bisect
import collections
import hashlib
import json
import math
from pathlib import Path

ROOT = Path('target/experiments')
OUT = Path('docs/experiments/residual-exposure-v1')

def read(p):
    return json.loads(p.read_text())

def rms(xs):
    xs = list(xs)
    return math.sqrt(sum(x*x for x in xs)/len(xs)) if xs else None

def exposure(rows):
    return dict(samples=len(rows), phases=dict(collections.Counter(r['phase'] for r in rows)),
        noisy_samples=sum(r['noise'] > 8 for r in rows),
        noisy_raw_exposed_samples=sum(r['noise'] > 8 and max(r['raw_slerp_weights']) > 0 for r in rows),
        max_raw_slerp_weight=max((max(r['raw_slerp_weights']) for r in rows), default=None),
        max_patch_gyro_weight=max((r['patch_gyro_weight'] for r in rows), default=None),
        max_light_partner_weight=max((r['light_partner_weight'] for r in rows), default=None),
        max_medium_handback_weight=max((r['medium_handback_weight'] for r in rows), default=None),
        patch_minus_optical_rms_deg_s=rms(r['patch_minus_optical_norm_deg_s'] for r in rows))

def main():
    paths = [ROOT/'residual-exposure-v1/0073-sources.json', ROOT/'bounce-localize-v1/localization.json',
             ROOT/'bounce-localize-v1/span-scores.json']
    raw = read(paths[0])['rows']
    times = [r['t'] for r in raw]
    assert all(a < b for a,b in zip(times,times[1:]))
    def subset(a,b):
        return raw[bisect.bisect_left(times,a):bisect.bisect_left(times,b)]
    image = [r for r in read(paths[1])['rows'] if r['reliable']]
    spans = {b['start']:b['span_difference_rms_deg_s'] for b in read(paths[2])['windows'][0]['bins']}
    groups = collections.defaultdict(list)
    for r in image:
        groups[math.floor(r['t']*4)/4].append(r)
    bins = []
    for t, rr in sorted(groups.items()):
        e = subset(t,t+.25)
        bins.append(dict(start=t, horizontal_low_px_s=rms(r['image_low'][0] for r in rr),
            horizontal_high_px_s=rms(r['image_high'][0] for r in rr),
            intended_low_deg_s=rms(math.sqrt(sum(x*x for x in r['intended_low_deg_s'])) for r in rr),
            span_difference_deg_s=spans.get(t), exposure=exposure(e),
            min_interior_margin_s=min((r['interior_margin_s'] for r in e),default=None)))
    guarded = []
    for margin in [0., .25, .5, 1.]:
        rr = []
        for r in image:
            j = bisect.bisect_left(times,r['t'])
            k = min([max(0,j-1),min(j,len(raw)-1)],key=lambda k:abs(times[k]-r['t']))
            e = raw[k]
            if abs(e['t']-r['t']) <= .001 and e['phase']=='rebased_interior' and e['interior_margin_s'] >= margin and e['patch_gyro_weight']==0:
                rr.append(r)
        guarded.append(dict(margin_s=margin,samples=len(rr),
            horizontal_low_px_s=rms(r['image_low'][0] for r in rr),
            horizontal_high_px_s=rms(r['image_high'][0] for r in rr),
            intended_low_deg_s=rms(math.sqrt(sum(x*x for x in r['intended_low_deg_s'])) for r in rr)))
    result = dict(windows=[dict(window=[a,b],**exposure(subset(a,b))) for a,b in [(14,30),(132,146),(55.9,58.9),(143.5,143.75)]],
        bins=bins,guarded_rebased_optical_only=guarded,
        note='Noise threshold 8 deg/s is detector evidence, not truth. Rate mixture and SLERP weights are distinct. Guard margins are sensitivity checks, not finite filter support. Clean control has no trace rows if no optical segment exists; missing is not zero.')
    (OUT/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    manifests = paths + [Path('o4core/examples/residual_exposure_probe.rs'),Path(__file__)]
    (OUT/'manifest.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in manifests},indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='bins'},indent=2))
    print('Largest low-intended-motion bins (intended < 0.5 deg/s):')
    for b in sorted((b for b in bins if b['intended_low_deg_s']<.5),key=lambda b:b['horizontal_low_px_s'],reverse=True)[:8]:
        print(json.dumps(b))

if __name__ == '__main__':
    main()
