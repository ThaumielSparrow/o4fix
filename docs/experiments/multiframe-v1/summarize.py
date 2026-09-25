"""Summarize finite iterates separately from converged solutions."""
import json,math,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
OUT=Path(__file__).resolve().parent
summary={}
for name in ['0060-control','0060-control-logdepth']:
    data=json.loads((ROOT/f'target/experiments/multiframe-v1/{name}.json').read_text())
    rows=data['rows'];valid=[r for r in rows if r['fit'] is not None]
    for r in valid:
        assert all(math.isfinite(x) for x in r['fit']['omega'])
    run=longest=0
    for r in rows:
        run=run+1 if r['fit'] is not None and r['fit']['converged'] else 0
        longest=max(longest,run)
    split=[r['split_disagreement_rad_s']*180/math.pi for r in valid if r['split_disagreement_rad_s'] is not None]
    item={'pairs':len(rows),'finite_iterates':len(valid),'converged':sum(r['fit']['converged'] for r in valid),
        'max_consecutive_converged_pairs':longest,
        'independent_fits_both_finite':sum(all(f is not None for f in r['independent']) for r in valid),
        'independent_fits_both_converged':sum(all(f is not None and f['converged'] for f in r['independent']) for r in valid),
        'median_split_disagreement_deg_s':statistics.median(split) if split else None,
        'median_cost_ratio':statistics.median(r['fit']['cost']/r['fit']['initial_cost'] for r in valid),
        'scores':json.loads((ROOT/f'target/experiments/multiframe-v1/{name}-scores.json').read_text())}
    summary[name]=item
    print(name,item)
(OUT/'metrics.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
