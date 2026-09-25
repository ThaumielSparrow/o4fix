"""Summarize lens inversion and held-out spatial diagnostics; no p-values."""
import json,math,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3];OUT=Path(__file__).resolve().parent
med=lambda v:statistics.median([x for x in v if x is not None]) if any(x is not None for x in v) else None
def corr(x,y):
    if len(x)<20:return None
    mx,my=statistics.mean(x),statistics.mean(y);xx=sum((v-mx)**2 for v in x);yy=sum((v-my)**2 for v in y)
    return sum((a-mx)*(b-my) for a,b in zip(x,y))/math.sqrt(xx*yy) if xx*yy>0 else None
summary={}
audit=json.loads((ROOT/'target/experiments/lens-v1/grid-audit.json').read_text());r=audit['rows']
summary['grid_audit']={'points':len(r),'sentinels':sum(v['opencv_sentinel'] for v in r),'roundtrip_over_0_1px':sum(v['opencv_roundtrip_px']>.1 for v in r),'roundtripping_other_branch':sum(v['opencv_beyond_first_branch'] and v['opencv_roundtrip_px']<=.1 for v in r),'principal_missing':sum(v['principal_normalized'] is None for v in r),'theta_limit_rad':audit['theta_limit'],'distorted_radius_limit':audit['distorted_radius_limit']}
summary['tracking']={}
for name in ['0021','0027','0060']:
    score=json.loads((ROOT/f'target/experiments/lens-v1/{name}-scores.json').read_text());folds=score['held_out_common_support']
    def pool(k):
        v=[f[k] for f in folds if f[k] is not None];n=sum(x['samples'] for x in v)
        return {'rms_deg_s':math.sqrt(sum(x['rms']**2*x['samples'] for x in v)/n),'samples':n}
    a,b=pool('baseline'),pool('candidate');assert a['samples']==b['samples']
    summary['tracking'][name]={'baseline':a,'principal_inverse':b,'relative_change_percent':100*(b['rms_deg_s']/a['rms_deg_s']-1),'folds':folds}
summary['spatial']={}
for name in ['0021','0027','0060']:
    data=json.loads((ROOT/f'target/experiments/lens-v1/{name}-residual.json').read_text());folds=[]
    for start,end in data['intervals']:
        rr=[r for r in data['rows'] if start<=r['t']<=end]
        out={'interval':[start,end],'pairs':len(rr),'evaluated_pairs':sum(r['evaluated']>=20 for r in rr),'median_epipolar_normal_px_half':med([r['median_magnitude_px_half'] for r in rr])}
        for key in ['row_dy_slope','radius_radial_slope','row_magnitude_correlation','radius_magnitude_correlation']:
            actual=med([r[key] for r in rr]);null=[med([r['shuffled_location'][i][key] for r in rr]) for i in range(32)];null=[x for x in null if x is not None]
            out[key]={'median':actual,'shuffled_median_range':[min(null),max(null)] if null else None}
        good=[r for r in rr if r['median_magnitude_px_half'] is not None]
        out['speed_error_correlation']=corr([r['angular_speed_deg_s'] for r in good],[r['median_magnitude_px_half'] for r in good])
        # Circular shifts preserve each series' temporal structure, though they are not independent replicates.
        speed=[r['angular_speed_deg_s'] for r in good];error=[r['median_magnitude_px_half'] for r in good]
        controls=[]
        for fraction in [.25,.5,.75]:
            shift=int(len(speed)*fraction);controls.append(corr(speed[shift:]+speed[:shift],error))
        out['speed_error_circular_shift_controls']=controls
        folds.append(out)
    summary['spatial'][name]=folds
(OUT/'metrics.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps(summary,indent=2))
