"""Quarter-second accepted-render diagnostics; run from repository root."""
import collections
import json
import math
from pathlib import Path

out = Path('target/experiments/bounce-localize-v1')
groups = collections.defaultdict(list)
for row in json.loads((out / 'localization.json').read_text())['rows']:
    if row['reliable']:
        groups[math.floor(row['t'] * 4) / 4].append(row)
bins = []
for t, rows in sorted(groups.items()):
    bins.append(dict(
        t=t,
        horizontal_low=math.sqrt(sum(r['image_low'][0]**2 for r in rows)/len(rows)),
        horizontal_high=math.sqrt(sum(r['image_high'][0]**2 for r in rows)/len(rows)),
        intended_rotation_low=math.sqrt(sum(sum(x*x for x in r['intended_low_deg_s']) for r in rows)/len(rows)),
        phases=dict(collections.Counter(r['phase'] for r in rows)),
        samples=len(rows),
    ))
(out / 'bins.json').write_text(json.dumps(bins, indent=2))
