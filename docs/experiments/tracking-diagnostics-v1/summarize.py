"""Assess unchanged-tracker diagnostics against frozen held-out clean references."""
import collections
import hashlib
import json
import math
from pathlib import Path
import runpy

stats=runpy.run_path('docs/experiments/optical-reliability-v1/summarize.py')
rho=stats['rho']; quantile=stats['quantile']; ranks=stats['ranks']; corr=stats['corr']
RAW=Path('target/experiments/tracking-diagnostics-v1')
OUT=Path('docs/experiments/tracking-diagnostics-v1')
FIELDS=['inliers','inlier_fraction','lk_error_median','sampson_median_normalized',
        'inlier_sampson_median_normalized','inlier_bbox_fraction','occupied_cells']
SIGN={'inliers':-1,'inlier_fraction':-1,'inlier_bbox_fraction':-1,'occupied_cells':-1}

def within_rho(rows,field):
    groups=collections.defaultdict(list)
    for r in rows:groups[(r['fold'],r['run'])].append(r)
    a=[]; b=[]
    for rr in groups.values():
        x=ranks([r[field] for r in rr]); y=ranks([r['error2'] for r in rr]); mid=(len(rr)-1)/2; scale=max(1,len(rr)-1)
        a.extend((v-mid)/scale for v in x); b.extend((v-mid)/scale for v in y)
    return corr(a,b)

def main():
    acquired={c:json.loads((RAW/f'{c}.json').read_text()) for c in ['0021','0027','0060','0073']}
    result={'parity':[], 'clips':{}, 'cross_clip':[], '0073_windows':[]}
    joined={}
    for clip,data in acquired.items():
        for v in data['variants']:
            expected=[]
            for lo,hi in data['intervals']:
                # These source clips and original caches are 100 fps.
                expected.extend((i-v['gap']*.5)/100 for i in range(int(max(lo*100,0))+v['gap'],int(hi*100)+2))
            assert len(expected)==len(v['t'])==len(v['omega'])==len(v['quality'])==len(v['diagnostics'])
            assert all(abs(x-y)<1e-9 for x,y in zip(expected,v['t']))
            result['parity'].append(dict(clip=clip,gap=v['gap'],pairs=len(expected),rate_max=v['max_rate_parity_rad_s'],quality_max=v['max_quality_parity']))
            for q,d in zip(v['quality'],v['diagnostics']):
                if q>0:
                    assert abs(q-max(0,min(1,(d['inliers']-60)/150)))<1e-12
                    assert sum(d['inlier_grid_counts'])==d['inliers']
                    assert d['inliers']<=d['tracked']<=d['detected']
        if clip=='0073':continue
        v=data['variants'][0]; lookup={round(t,9):d for t,d in zip(v['t'],v['diagnostics'])}
        rr=json.loads(Path(f'target/experiments/optical-reliability-v1/{clip}.json').read_text())['rows']
        for r in rr:
            d=lookup[round(r['t'],9)]
            for f in FIELDS:
                assert d.get(f) is not None and math.isfinite(d[f]),(clip,r['t'],f)
                r[f]=d[f]*SIGN.get(f,1)
        # A predeclared +/-0.10s median reduces per-pair metric noise. No filtering across run boundaries.
        for r in rr:
            local=[p for p in rr if p['fold']==r['fold'] and p['run']==r['run'] and abs(p['t']-r['t'])<=.100000001]
            for f in FIELDS:r[f+'_local']=quantile([p[f] for p in local],.5)
        joined[clip]=rr
    fields=FIELDS+[f+'_local' for f in FIELDS]+['optical_speed']
    for clip,rr in joined.items():
        result['clips'][clip]={'samples':len(rr),'signals':{}}
        train=[r for c,rows in joined.items() if c!=clip for r in rows]
        errcut=quantile([r['error2'] for r in train],.8); bad=[r['error2']>errcut for r in rr]
        for f in fields:
            groups=collections.defaultdict(list)
            for r in rr:groups[(r['fold'],r['run'])].append(r)
            shifted=[]
            for fraction in [.25,.5,.75]:
                a=[]; b=[]; orig=[]
                for run in groups.values():
                    k=round(len(run)*fraction)
                    if min(k,len(run)-k)<50:continue
                    a.extend(r[f] for r in run);orig.extend(r['error2'] for r in run)
                    b.extend(run[(i+k)%len(run)]['error2'] for i in range(len(run)))
                shifted.append(dict(fraction=fraction,rho=rho(a,b),same_support_rho=rho(a,orig)))
            result['clips'][clip]['signals'][f]=dict(rho=rho([r[f] for r in rr],[r['error2'] for r in rr]),within_run_rho=within_rho(rr,f),shifted=shifted)
            cutoff=quantile([r[f] for r in train],.8);flagged=[r[f]>cutoff for r in rr]; n=sum(flagged); tp=sum(a and b for a,b in zip(flagged,bad))
            result['cross_clip'].append(dict(clip=clip,signal=f,threshold=cutoff,error_threshold=errcut,flagged_fraction=n/len(rr),
                precision=tp/n if n else None,recall=tp/sum(bad),prevalence=sum(bad)/len(rr)))
    for lo,hi in [(132,146),(55.9,58.9),(142.5,142.75),(143,143.25),(143.25,143.5),(143.5,143.75)]:
        for v in acquired['0073']['variants']:
            ds=[d for t,d in zip(v['t'],v['diagnostics']) if lo<=t<hi]
            result['0073_windows'].append(dict(window=[lo,hi],gap=v['gap'],samples=len(ds),
                quantiles={f:[quantile([d[f] for d in ds if d.get(f) is not None],q) for q in [.1,.5,.9]] for f in FIELDS}))
    result['notes']=['All predictors exploratory; no validation clip used to set its threshold. Thresholds are diagnostics, not proposed rejection gates.',
        'Low count/fraction/coverage have negative signs so larger signal means hypothesized higher risk; errors, speed retain positive signs.',
        'Pair diagnostics compared to filtered 5Hz error; raw and +/-0.10s within-run median versions both retained.',
        'Fit residuals use the same fitted essential matrix and inlier set: optimistic, not independent held-out error. Normalized Sampson values are unsquared distances under existing inversion, not pixel uncertainty.',
        'Temporal samples are dependent; circular controls are sensitivity checks, not p-values. Multi-signal exploration needs new validation before tuning.',
        '0073 severe gyro is not used as truth; only diagnostic distributions are reported.']
    (OUT/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    paths=list(RAW.glob('*.json'))+[Path(__file__),Path('o4core/examples/tracking_diagnostic_probe.rs'),Path('o4core/examples/support/diagnostic_tracker.rs')]
    paths += [Path(f'target/experiments/optical-reliability-v1/{c}.json') for c in joined]
    paths += [Path('target/experiments/bounce-localize-v1/spans.json')]
    (OUT/'manifest.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2)+'\n')
    print('Parity:',result['parity'])
    for f in fields:
        print(f,[(c,round(d['signals'][f]['rho'] or 0,3),round(d['signals'][f]['within_run_rho'] or 0,3)) for c,d in result['clips'].items()])

if __name__=='__main__':main()
