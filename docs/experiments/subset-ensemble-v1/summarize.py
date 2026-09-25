"""Preserve compact verification and immutable artifact fingerprints after rendering."""
import hashlib
import json
from pathlib import Path
out=Path('target/experiments/subset-ensemble-v1')
docs=Path('docs/experiments/subset-ensemble-v1')
for name in ['candidate-bursts.json','event-rates.json','review-validation.json']:
    if (out/name).exists():(docs/name).write_text(json.dumps(json.loads((out/name).read_text()),indent=2)+'\n')
def digest(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
    return dict(bytes=p.stat().st_size,sha256=h.hexdigest())
paths=[Path('o4core/examples/subset_ensemble_repair.rs'),Path('o4core/examples/support/ensemble_tracker.rs'),
       Path('o4core/examples/support/subset_tracker.rs'),Path('o4core/examples/support/subset_rotation.rs'),
       Path('o4core/examples/support/gap_patch.rs'),Path('o4core/examples/support/edge_offset_patch.rs'),
       Path('target/release/examples/subset_ensemble_repair.exe')]
paths+=list(docs.glob('*.py'))
paths += [out/n for n in ['ensemble.MP4','ensemble.gyroflow','ensemble-render.mp4','compare_early.mp4','compare_late.mp4','compare_control.mp4','compare_additional.mp4','repair.log','render.log'] if (out/n).exists()]
(docs/'manifest.json').write_text(json.dumps(dict(files={str(p):digest(p) for p in paths},
    prior_input_manifests=['docs/experiments/bounce-localize-v1/manifest.json','docs/experiments/subset-stability-v1/manifest.json']),indent=2)+'\n')
