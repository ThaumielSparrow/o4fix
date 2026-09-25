import json, math, pathlib
root=pathlib.Path.cwd()
d=json.loads((root/'target/experiments/gyro-trace-v1/trace.json').read_text())
intervals=json.loads((root/'target/experiments/splice-exposure-v1/exposure.json').read_text())['bursts']
summary=[]
for b in intervals:
 a,e=b['snapped_interval']
 rows=[r for r in d['rows'] if a<=r['t']<=e]
 diffs=[math.sqrt(sum((r['optical_weight']*r['handback']*(r['medium_deg_s'][k]-r['optical_deg_s'][k]))**2 for k in range(3))) for r in rows]
 summary.append({'interval':[a,e], 'samples':len(rows), 'mean_handback':sum(r['handback'] for r in rows)/len(rows), 'max_handback':max(r['handback'] for r in rows), 'handback_added_rate_rms_deg_s':math.sqrt(sum(x*x for x in diffs)/len(diffs)), 'handback_added_rate_max_deg_s':max(diffs), 'peak_difference_time':rows[diffs.index(max(diffs))]['t']})
(root/'docs/experiments/gyro-trace-v1/summary.json').write_text(json.dumps(summary,indent=2))
print(json.dumps(summary,indent=2))
