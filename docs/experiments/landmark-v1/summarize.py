"""Summarize free-landmark fits and matched initialization sensitivity."""
import json,statistics,math,collections
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
OUT=Path(__file__).resolve().parent
summary={};saved={}
for name in ['default','perturbed']:
    d=json.loads((ROOT/f'target/experiments/landmark-v1/{name}.json').read_text())
    rows=d['rows'];saved[name]=rows;valid=[r['fit'] for r in rows if r['fit'] is not None]
    for f in valid:assert all(math.isfinite(x) for x in f['omega'])
    run=longest=0
    for r in rows:
        run=run+1 if r['fit'] is not None and r['fit']['converged'] else 0
        longest=max(longest,run)
    summary[name]={'pairs':len(rows),'finite_iterates':len(valid),'converged':sum(f['converged'] for f in valid),
        'longest_converged_run_pairs':longest,'rank_histogram':dict(collections.Counter(f['observability']['rank_at_1e_8_relative'] for f in valid)),
        'median_second_eigenvalue_ratio':statistics.median(f['observability']['second_eigenvalue_ratio'] for f in valid),
        'max_preprojection_scale_residual_ratio':max(f['observability']['preprojection_scale_residual_ratio'] for f in valid),
        'median_objective_ratio':statistics.median(f['cost']/f['initial_cost'] for f in valid),
        'scores':json.loads((ROOT/f'target/experiments/landmark-v1/{name}-scores.json').read_text())}
sensitivity={}
assert [r['t'] for r in saved['default']]==[r['t'] for r in saved['perturbed']]
for converged in [False,True]:
    pairs=[(a['fit'],b['fit']) for a,b in zip(saved['default'],saved['perturbed']) if a['fit'] and b['fit'] and (not converged or (a['fit']['converged'] and b['fit']['converged']))]
    error=sorted(math.sqrt(sum((x-y)**2 for x,y in zip(a['omega'],b['omega'])))*180/math.pi for a,b in pairs)
    sensitivity['both_converged' if converged else 'both_finite']={'pairs':len(error),'median_deg_s':statistics.median(error),'p90_deg_s':error[int(.9*(len(error)-1))],'max_deg_s':max(error)}
summary['initialization_sensitivity']=sensitivity
(OUT/'metrics.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
print('Summary saved.')
