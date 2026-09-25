import json,math,pathlib
root=pathlib.Path.cwd()
for clip,windows in [('0071',[(11,21),(252,260)]),('0073',[(14,30),(132,146)])]:
 out=root/'target/experiments/residual-stage-v1'/clip
 stages=json.loads((out/'stages.json').read_text())
 trace=json.loads((out/'trace.json').read_text())['rows']
 summary=[]
 for burst in stages['bursts']:
  a,b=burst['interval']
  if not any(a<end and b>start for start,end in windows):continue
  rows=[r for r in trace if a<=r['t']<=b]
  delta=[math.sqrt(sum((r['optical_weight']*r['handback']*(r['medium_deg_s'][k]-r['optical_deg_s'][k]))**2 for k in range(3))) for r in rows]
  burst['handback_added_rate_rms_deg_s']=math.sqrt(sum(v*v for v in delta)/len(delta))
  burst['handback_added_rate_peak_deg_s']=max(delta)
  burst['max_handback_weight']=max(r['handback'] for r in rows)
  burst['peak_noise_deg_s']=max(r['noise'] for r in rows)
  summary.append(burst)
 dest=root/'docs/experiments/residual-stage-v1'/clip;dest.mkdir(exist_ok=True)
 (dest/'summary.json').write_text(json.dumps({'accepted_parity_max_component':stages['parity_max_component'],'bursts':summary},indent=2))
 print(clip)
 for b in summary:print(b)
