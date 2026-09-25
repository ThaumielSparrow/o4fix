"""Frozen clean-reference reliability study; no fitted correction or new tracker."""
import collections
import hashlib
import json
import math
from pathlib import Path

OUT=Path('docs/experiments/optical-reliability-v1')
RAW=Path('target/experiments/optical-reliability-v1')

def quantile(x,q):
    x=sorted(x)
    if not x:return None
    z=q*(len(x)-1); lo=int(z); hi=min(lo+1,len(x)-1)
    return x[lo]+(z-lo)*(x[hi]-x[lo])

def ranks(x):
    order=sorted(range(len(x)),key=x.__getitem__); result=[0.]*len(x); i=0
    while i<len(x):
        j=i+1
        while j<len(x) and x[order[j]]==x[order[i]]:j+=1
        for k in order[i:j]:result[k]=(i+j-1)/2
        i=j
    return result

def corr(a,b):
    if len(a)<3:return None
    ma=sum(a)/len(a); mb=sum(b)/len(b)
    va=sum((v-ma)**2 for v in a); vb=sum((v-mb)**2 for v in b)
    return sum((x-ma)*(y-mb) for x,y in zip(a,b))/math.sqrt(va*vb) if va*vb>0 else None

def rho(a,b):return corr(ranks(a),ranks(b))
def rms(x):return math.sqrt(sum(v*v for v in x)/len(x)) if x else None
PREDICTORS={'span':lambda r:r['span_difference'],'low_quality1':lambda r:1-r['quality1'],
            'low_quality2':lambda r:1-r['quality2'],'speed':lambda r:r['optical_speed']}

def summary(rows):
    return dict(samples=len(rows),baseline_rms=rms([r['error1'] for r in rows]),gap2_rms=rms([r['error2'] for r in rows]),
        quality1_quantiles=[quantile([r['quality1'] for r in rows],q) for q in [0,.2,.5,.8,1]],
        quality2_quantiles=[quantile([r['quality2'] for r in rows],q) for q in [0,.2,.5,.8,1]],
        span_quantiles=[quantile([r['span_difference'] for r in rows],q) for q in [.2,.5,.8]],
        within_run_span_error2_rho=within_run_rho(rows),
        correlations={k:{e:rho([fn(r) for r in rows],[r[e] for r in rows]) for e in ['error1','error2']} for k,fn in PREDICTORS.items()})

def within_run_rho(rows):
    groups=collections.defaultdict(list)
    for r in rows:groups[(r['fold'],r['run'])].append(r)
    a=[]; b=[]
    for rr in groups.values():
        x=ranks([r['span_difference'] for r in rr]); y=ranks([r['error2'] for r in rr])
        scale=max(1,len(rr)-1); mid=(len(rr)-1)/2
        a.extend((v-mid)/scale for v in x); b.extend((v-mid)/scale for v in y)
    return corr(a,b)

def shifted_controls(rows):
    groups=collections.defaultdict(list)
    for r in rows:groups[(r['fold'],r['run'])].append(r)
    result=[]
    for fraction in [.25,.375,.5,.625,.75]:
        a=[]; b=[]; original=[]
        for rr in groups.values():
            k=round(len(rr)*fraction)
            if min(k,len(rr)-k)<50:continue
            a.extend(r['span_difference'] for r in rr)
            original.extend(r['error2'] for r in rr)
            b.extend(rr[(i+k)%len(rr)]['error2'] for i in range(len(rr)))
        result.append(dict(fraction=fraction,samples=len(a),rho=rho(a,b),unshifted_same_support_rho=rho(a,original)))
    return result

def main():
    data={c:json.loads((RAW/f'{c}.json').read_text()) for c in ['0021','0027','0060']}
    result={'clips':{c:summary(d['rows']) for c,d in data.items()},'folds':[], 'cross_clip':[],
            'time_shift_controls':{c:shifted_controls(d['rows']) for c,d in data.items()}}
    for c,d in data.items():
        for i,f in enumerate(d['folds']):
            rr=[r for r in d['rows'] if r['fold']==i]
            assert len(rr)==f['baseline']['samples']==f['gap2']['samples']
            assert abs(rms([r['error1'] for r in rr])-f['baseline']['rms'])<1e-12
            assert abs(rms([r['error2'] for r in rr])-f['gap2']['rms'])<1e-12
            result['folds'].append(dict(clip=c,interval=f['interval'],**summary(rr)))
        train=[r for other,dd in data.items() if other!=c for r in dd['rows']]
        test=d['rows']; threshold=quantile([r['error2'] for r in train],.8)
        bad=[r['error2']>threshold for r in test]
        for name,fn in PREDICTORS.items():
            cutoff=quantile([fn(r) for r in train],.8)
            flagged=[fn(r)>cutoff for r in test]
            tp=sum(a and b for a,b in zip(flagged,bad)); n=sum(flagged)
            result['cross_clip'].append(dict(test_clip=c,predictor=name,train_error2_cutoff=threshold,
                train_predictor_cutoff=cutoff,test_samples=len(test),flagged=n,large_errors=sum(bad),
                precision=tp/n if n else None,recall=tp/sum(bad) if sum(bad) else None,
                prevalence=sum(bad)/len(test), retained_gap2_rms=rms([r['error2'] for r,f in zip(test,flagged) if not f])))
    spans=json.loads(Path('target/experiments/bounce-localize-v1/spans.json').read_text())
    result['0073_quality']=[]
    for lo,hi in [(132,146),(55.9,58.9),(143,143.75)]:
        for v in spans['variants']:
            q=[q for t,q in zip(v['t'],v['quality']) if lo<=t<hi]
            result['0073_quality'].append(dict(window=[lo,hi],gap=v['gap'],samples=len(q),
                below_03=sum(x<.3 for x in q),above_05=sum(x>.5 for x in q),
                quantiles=[quantile(q,z) for z in [0,.2,.5,.8,1]]))
    result['notes']=['5Hz clean-reference errors reproduce existing tracking scores; no new calibration.',
        'Common quality>0.5 selection restricts quality range. Samples are temporally dependent; sections/clips, not frames, are replication units.',
        'Disagreement shares estimator noise with error: association is not independent accuracy or calibrated uncertainty.',
        'Cross-clip quintile thresholds are diagnostic only; rejecting rates would create unsupported gaps and is not a repair strategy.',
        'Shift controls preserve within-run values but circular wrapping is artificial; not p-values.',
        '0073 severe-window quality has no trustworthy gyro-reference error. Its LP8 span scores are not directly comparable to these LP5 thresholds.']
    (OUT/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    paths=list(RAW.glob('*.json'))+[Path(__file__),Path('o4core/examples/optical_reliability_score.rs'),Path('o4core/examples/support/reliability_alignment.rs'),Path('target/experiments/bounce-localize-v1/spans.json')]
    for c,obs in [('0021','0021-gap.json'),('0027','0027-gap.json'),('0060','gap-observations.json')]:
        paths.extend([Path(f'target/experiments/calibration-v1/{c}/calibration.json'),Path(f'target/experiments/tracking-v1/{obs}')])
    (OUT/'manifest.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ['folds','notes']},indent=2))

if __name__=='__main__':main()
