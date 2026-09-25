"""Preserve compact experiment results and fingerprints, after acquisition."""
import collections
import hashlib
import json
from pathlib import Path

out=Path('target/experiments/partner-gate-v1')
docs=Path('docs/experiments/partner-gate-v1')
c=json.loads((out/'candidate.json').read_text())
changes=c.pop('changes')
groups=collections.defaultdict(list)
for r in changes:
    groups[int(r['t']*4)/4].append(r)
c['change_bins']=[dict(start=t,samples=len(rr),max_delta_deg_s=max(r['delta_deg_s'] for r in rr),
    max_handback=max(r['handback'] for r in rr),max_old_light_weight=max(r['old_light_weight'] for r in rr)) for t,rr in sorted(groups.items())]
(docs/'candidate-summary.json').write_text(json.dumps(c,indent=2)+'\n')
for name in ['event-rates.json','review-validation.json']:
    if (out/name).exists():
        (docs/name).write_text(json.dumps(json.loads((out/name).read_text()),indent=2)+'\n')

def digest(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''):
            h.update(chunk)
    return dict(bytes=p.stat().st_size,sha256=h.hexdigest())

paths=[Path('o4core/examples/partner_gate_repair.rs'),Path('o4core/examples/support/partner_gate.rs'),
    Path('o4core/examples/support/edge_offset_patch.rs'),Path('target/release/examples/partner_gate_repair.exe'),
    Path('target/experiments/residual-stage-v1/0073/trace.json'),
    Path('target/experiments/generalization-v1/0073/edgeoffset.MP4'),
    Path('sample_vids/DJI_20260829141435_0073_D.MP4')]
paths += list(docs.glob('*.py'))
paths += [out/n for n in ['partnergate.MP4','partnergate.gyroflow','candidate.json','partnergate-render.mp4','compare_early.mp4','compare_late.mp4','compare_control.mp4','compare_additional.mp4','repair.log','render.log'] if (out/n).exists()]
(docs/'manifest.json').write_text(json.dumps({str(p):digest(p) for p in paths},indent=2)+'\n')
