import json,random
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
D=ROOT/'target/experiments/render-selective-v1'
stages=json.loads((ROOT/'target/experiments/residual-stage-v1/0073/stages.json').read_text())['bursts']
for name,start in [('late',132),('control',55.9),('additional',264)]:
    data=json.loads((ROOT/f'target/experiments/render-rotation-v1/{name}-probe.json').read_text())
    events=[];weights=[0.]*data['frames']
    for stage in stages:
        lo,hi=stage['interval']
        rows=[r for r in data['rows'] if lo<=r['t']<=hi and r['score']]
        if not rows:continue
        blocks={}
        for r in rows:
            s=r['score'];b=blocks.setdefault(int((r['t']-lo)*2),[0.,0.])
            b[0]+=s['region_before_ss'];b[1]+=s['region_before_ss']-s['roll_region_after_ss']
        vals=list(blocks.values());rng=random.Random(271828);samples=[]
        for _ in range(4000):
            draw=rng.choices(vals,k=len(vals));samples.append(sum(b[1] for b in draw)/sum(b[0] for b in draw))
        samples.sort();low,high=samples[100],samples[3900]
        complete=lo>=start and hi<=start+data['frames']/100.
        accept=complete and len(vals)>=5 and low>0
        events.append({'interval':[lo,hi],'blocks':len(vals),'samples':len(rows),'bootstrap_interval':[low,high],'accepted':accept})
        if accept:
            for i in range(len(weights)):
                t=start+i/100.
                v=max(0.,min(1.,(t-lo)/0.2,(hi-t)/0.2))
                weights[i]=max(weights[i],v*v*(3-2*v))
    output={'weights':weights,'events':events,'start':start,'minimum_half_second_blocks':5,'fade_seconds':0.2,'bootstrap_seed':271828,'note':'Exploratory burst-level eligibility rule; block resampling is a stability check, not calibrated probability of perceptual benefit. Known review windows used in development; no production promotion.'}
    p=D/f'{name}-gate.json'
    if p.exists():raise RuntimeError('Refusing overwrite '+str(p))
    p.write_text(json.dumps(output,indent=2),encoding='utf-8')
    print(name,[(e['interval'],e['accepted']) for e in events])
