"""Frozen clean-reference test of independent feature-subset rotation disagreement."""
import collections
import hashlib
import json
import math
from pathlib import Path
import runpy

stats=runpy.run_path('docs/experiments/optical-reliability-v1/summarize.py')
rho=stats['rho'];quantile=stats['quantile'];corr=stats['corr'];ranks=stats['ranks']
RAW=Path('target/experiments/subset-stability-v1')
OUT=Path('docs/experiments/subset-stability-v1')

def within(rows,key):
    groups=collections.defaultdict(list)
    for r in rows:groups[(r['fold'],r['run'])].append(r)
    x=[];y=[]
    for rr in groups.values():
        a=ranks([r[key] for r in rr]); b=ranks([r['error2'] for r in rr]); mid=(len(rr)-1)/2;scale=max(1,len(rr)-1)
        x.extend((v-mid)/scale for v in a);y.extend((v-mid)/scale for v in b)
    return corr(x,y)

def shifts(rows,key):
    groups=collections.defaultdict(list)
    for r in rows:groups[(r['fold'],r['run'])].append(r)
    out=[]
    for fraction in [.25,.5,.75]:
        a=[];b=[];orig=[]
        for rr in groups.values():
            k=round(len(rr)*fraction)
            if min(k,len(rr)-k)<50:continue
            a.extend(r[key] for r in rr);orig.extend(r['error2'] for r in rr)
            b.extend(rr[(i+k)%len(rr)]['error2'] for i in range(len(rr)))
        out.append(dict(fraction=fraction,samples=len(a),rho=rho(a,b),same_support_rho=rho(a,orig)))
    return out

def diagnostic_rows(v):
    out={}
    for t,d in zip(v['t'],v['diagnostics']):
        s=d.get('subset',{})
        angle=s.get('angle_rad')
        if angle is None:out[round(t,9)]=None;continue
        h=s['halves'];dt=v['gap']/100
        assert sum(x['tracks'] for x in h)==d['tracked']
        assert all(x['inliers']>=60 and x['inliers']<=x['tracks'] for x in h)
        out[round(t,9)]=dict(t=t,split_deg_s=math.degrees(angle)/dt,
            full_delta_deg_s=max(math.degrees(x['full_angle_rad'])/dt for x in h),
            minimum_half_inliers=min(x['inliers'] for x in h))
    return out

def main():
    inputs={c:json.loads((RAW/f'{c}.json').read_text()) for c in ['0021','0027','0060','0073']}
    result={'parity':[],'clips':{},'cross_clip':[],'0073_windows':[],'0073_bins':[]};joined={}
    for c,data in inputs.items():
        for v in data['variants']:
            expected=[]
            for lo,hi in data['intervals']:
                expected.extend((i-v['gap']*.5)/100 for i in range(int(max(lo*100,0))+v['gap'],int(hi*100)+2))
            assert len(v['t'])==len(expected)==len(v['diagnostics'])
            assert all(abs(a-b)<1e-9 for a,b in zip(expected,v['t']))
            assert v['max_rate_parity_rad_s']<=1e-12 and v['max_quality_parity']<=1e-12
            lookup=diagnostic_rows(v)
            result['parity'].append(dict(clip=c,gap=v['gap'],pairs=len(expected),valid_subsets=sum(x is not None for x in lookup.values()),rate_max=v['max_rate_parity_rad_s'],quality_max=v['max_quality_parity']))
        if c=='0073':continue
        ref=json.loads(Path(f'target/experiments/optical-reliability-v1/{c}.json').read_text())['rows']
        lookup=diagnostic_rows(data['variants'][0]);rr=[]
        for r in ref:
            d=lookup[round(r['t'],9)]
            if d is not None:rr.append(dict(r,**{k:v for k,v in d.items() if k!='t'}))
        for r in rr:
            local=[p for p in rr if p['fold']==r['fold'] and p['run']==r['run'] and abs(p['t']-r['t'])<=.100000001]
            r['split_local']=quantile([p['split_deg_s'] for p in local],.5)
        joined[c]=rr
        result['clips'][c]=dict(reference_samples=len(ref),supported_samples=len(rr),
            split_quantiles=[quantile([r['split_deg_s'] for r in rr],q) for q in [.1,.5,.9,.99]],
            signals={k:dict(rho=rho([r[k] for r in rr],[r['error2'] for r in rr]),within_run_rho=within(rr,k),shifts=shifts(rr,k)) for k in ['split_deg_s','split_local','full_delta_deg_s']},
            folds=[dict(fold=f,samples=len([r for r in rr if r['fold']==f]),rho=rho([r['split_local'] for r in rr if r['fold']==f],[r['error2'] for r in rr if r['fold']==f])) for f in sorted(set(r['fold'] for r in rr))])
    for c,rr in joined.items():
        train=[r for other,rs in joined.items() if other!=c for r in rs]
        cutoff_error=quantile([r['error2'] for r in train],.8);bad=[r['error2']>cutoff_error for r in rr]
        for key in ['split_deg_s','split_local','full_delta_deg_s']:
            cutoff=quantile([r[key] for r in train],.8);flagged=[r[key]>cutoff for r in rr];n=sum(flagged);tp=sum(x and y for x,y in zip(flagged,bad))
            result['cross_clip'].append(dict(clip=c,signal=key,threshold=cutoff,error_threshold=cutoff_error,flagged_fraction=n/len(rr),precision=tp/n if n else None,recall=tp/sum(bad),prevalence=sum(bad)/len(rr)))
    for v in inputs['0073']['variants']:
        lookup=diagnostic_rows(v)
        for lo,hi in [(132,146),(55.9,58.9),(142.5,142.75),(143,143.25),(143.25,143.5),(143.5,143.75)]:
            values=[r for t,r in lookup.items() if lo<=t<hi];valid=[r for r in values if r is not None]
            result['0073_windows'].append(dict(window=[lo,hi],gap=v['gap'],pairs=len(values),valid=len(valid),split_quantiles=[quantile([r['split_deg_s'] for r in valid],q) for q in [.1,.5,.9]]))
        for bin in range(56):
            lo=132+bin/4;hi=lo+.25;vs=[r for t,r in lookup.items() if lo<=t<hi and r is not None]
            result['0073_bins'].append(dict(start=lo,gap=v['gap'],valid=len(vs),split_median=quantile([r['split_deg_s'] for r in vs],.5)))
    result['notes']=['Independent here means disjoint correspondences, not independent image noise, model bias, or camera evidence.',
        'Same essential estimator and minimum 60 inliers per subset; missing subsets are unavailable, not zero disagreement.',
        'Half rotations use exact relative-rotation angle divided by pair span; diagnostics are raw or +/-0.10s median, reference errors are 5Hz filtered.',
        'Circular controls are sensitivity checks on dependent samples, not p-values. Cross-clip thresholds use only other clips.',
        'Synthetic shared-bias control proves that subset agreement cannot certify accuracy. No severe gyro-reference error is asserted.']
    result['ensemble_scores']={}
    for c in joined:
        p=RAW/f'{c}-ensemble-scores.json'
        if p.exists():
            scores=json.loads(p.read_text())['held_out_common_support']
            pooled={k:math.sqrt(sum(f[k]['rms']**2*f[k]['samples'] for f in scores)/sum(f[k]['samples'] for f in scores)) for k in ['baseline','candidate']}
            old=json.loads(Path(f'docs/experiments/optical-reliability-v1/summary.json').read_text())['clips'][c]['gap2_rms']
            assert abs(old-pooled['baseline'])<1e-12
            result['ensemble_scores'][c]=dict(full_gap2_rms=pooled['baseline'],ensemble_rms=pooled['candidate'],change_percent=100*(pooled['candidate']/pooled['baseline']-1),
                improved_sections=sum(f['candidate']['rms']<f['baseline']['rms'] for f in scores),sections=len(scores),folds=scores)
    (OUT/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    paths=list(RAW.glob('*.json'))+[Path(__file__),Path('o4core/examples/subset_stability_probe.rs'),Path('o4core/examples/support/subset_tracker.rs'),Path('o4core/examples/support/subset_rotation.rs'),Path('o4core/examples/subset_ensemble_cache.rs')]
    paths += [Path(f'target/experiments/optical-reliability-v1/{c}.json') for c in joined]
    paths += [Path(f'target/experiments/tracking-diagnostics-v1/{c}-input.json') for c in joined]
    paths += [Path('target/experiments/bounce-localize-v1/spans.json')]
    (OUT/'manifest.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ['0073_bins','notes']},indent=2))

if __name__=='__main__':main()
