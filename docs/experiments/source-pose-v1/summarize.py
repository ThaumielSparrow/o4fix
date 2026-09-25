"""Compact source-pose results, preserving common-support counts and null scores."""
import json, math, statistics
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent

def pooled(scores):
    values=[s for s in scores if s is not None]
    n=sum(s['samples'] for s in values)
    return {'rms_deg_s': math.sqrt(sum(s['rms_deg_s']**2*s['samples'] for s in values)/n) if n else None,
            'samples': n, 'scored_folds':len(values)}

summary={}
for name in ['0021','0027','0060']:
    d=json.loads((ROOT/f'target/experiments/source-pose-v1/{name}.json').read_text())
    rows=d['rows']
    assert all(len(r['models'])==4 for r in rows)
    for r in rows:
        for m in r['models']:
            if m is not None: assert all(math.isfinite(x) for x in m['omega'])
    item={'metadata':d['metadata'],'pairs':len(rows),'folds':d['folds'],'comparisons':[],'models':[]}
    for index in [0,2,3]:
        entries=[next(c for c in f['comparisons'] if c['candidate_index']==index) for f in d['folds']]
        a=pooled([c['reference'] for c in entries]);b=pooled([c['candidate'] for c in entries])
        assert a['samples']==b['samples']
        item['comparisons'].append({'candidate':d['model_order'][index],'reference':a,'candidate_score':b,
            'relative_change_percent':100*(b['rms_deg_s']/a['rms_deg_s']-1) if a['rms_deg_s'] and b['rms_deg_s'] is not None else None,
            'common_pairs':sum(c['common_pairs'] for c in entries)})
    for i,label in enumerate(d['model_order']):
        valid=sum(r['models'][i] is not None for r in rows)
        split=[r['diagnostics'][i]['split_rotation_disagreement_rad']*180/math.pi*100 for r in rows if i<3 and r['diagnostics'][i]['split_rotation_disagreement_rad'] is not None]
        item['models'].append({'name':label,'valid_pairs':valid,'missing_pairs':len(rows)-valid,'split_valid_pairs':len(split),'median_split_deg_s':statistics.median(split) if split else None})
    depth=json.loads((ROOT/f'target/experiments/source-pose-v1/{name}-depth.json').read_text())['rows']
    item['depth_check']={'original_valid':sum(r['original_valid'] for r in depth),
        'relaxed_valid':sum(r['relaxed_valid'] for r in depth),
        'rotation_disagreements_over_1e_5_rad':sum(r['rotation_disagreement_rad'] is not None and r['rotation_disagreement_rad']>1e-5 for r in depth)}
    summary[name]=item
    print(name,item['pairs'])
    for c in item['comparisons']:print(c)
    print(item['models'])
(OUT/'metrics.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
