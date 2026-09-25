import concurrent.futures,hashlib,json,math,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
D=ROOT/'target/experiments/render-selective-v1'
DOC=Path(__file__).resolve().parent
checks=[]
for exp in ['render-rotation-v1','render-regional-v1','render-bounce-v1','render-selective-v1']:
    for name,count in [('late',1400),('control',300),('additional',800)]:
        for suffix,w,h in [('_full',2880,810),('',1440,406)]:
            checks.append((ROOT/f'target/experiments/{exp}/compare_{name}{suffix}.mp4',count,w,h))
checks.extend([(D/'compare_focus.mp4',500,1440,406),(D/'compare_detail.mp4',500,1280,360)])
def video_check(job):
    p,count,w,h=job
    r=subprocess.run(['C:/ffmpeg/bin/ffprobe.exe','-v','error','-count_frames','-select_streams','v:0','-show_entries','stream=width,height,r_frame_rate,nb_read_frames','-of','json',str(p)],capture_output=True,text=True,check=True)
    v=json.loads(r.stdout)['streams'][0]
    assert (v['width'],v['height'],v['r_frame_rate'],int(v['nb_read_frames']))==(w,h,'100/1',count),(p,v)
    return str(p.relative_to(ROOT)),v
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    videos=dict(pool.map(video_check,checks))
summary={'video_checks':videos,'windows':{}}
for name in ['late','control','additional']:
    gate=json.loads((D/f'{name}-gate.json').read_text())
    params=json.loads((D/f'compare_{name}_full.mp4.json').read_text())
    assert len(params['angles'])==len(gate['weights'])
    selected=[e['interval'] for e in gate['events'] if e['accepted']]
    for i,(angle,w) in enumerate(zip(params['angles'],gate['weights'])):
        t=gate['start']+i/100.
        if not any(lo<t<hi for lo,hi in selected):assert w==0 and angle==0
    if name!='late':assert all(a==0 for a in params['angles'])
    assert params['source_margin']>4 and params['max_angle_rad']<=math.radians(.5)
    bands=json.loads((D/f'{name}-bands.json').read_text())
    change=[{k:100*(a[k]/b[k]-1) for k in ['translation_rms_half_px_s','roll_rms_deg_s']} for b,a in zip(bands['panels'][0]['bands'],bands['panels'][1]['bands'])]
    summary['windows'][name]={'events':gate['events'],'max_angle_deg':math.degrees(params['max_angle_rad']),'source_margin_px':params['source_margin'],'nonzero_frames':sum(a!=0 for a in params['angles']),'band_changes':change}
rows=json.loads((D/'late-bins.json').read_text())['panels']
summary['selected_burst_changes']=[]
for b,a in zip(rows[0]['bands'],rows[1]['bands']):
    pairs=[(x,y) for x,y in zip(b['bins'],a['bins']) if 141.25<=x['t']<144.5]
    summary['selected_burst_changes'].append({'hz':b['hz'],**{k:100*(math.sqrt(sum(y[k]**2*y['n'] for x,y in pairs)/sum(x[k]**2*x['n'] for x,y in pairs))-1) for k in ['xy','roll']}})
(DOC/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
files=[p for p,_,_,_ in checks]
for exp in ['render-rotation-v1','render-regional-v1','render-bounce-v1','render-selective-v1']:
    files+=list((ROOT/'target/experiments'/exp).glob('*.json'))
    files+=list((ROOT/'docs/experiments'/exp).glob('*.py'))
names=['render_rotation_probe','render_rotation_review','render_regional_review','render_bounce_review','render_band_diagnostic','render_window_measure','render_selective_review']
files += [ROOT/'o4core/examples'/f'{n}.rs' for n in names]+[ROOT/'target/release/examples'/f'{n}.exe' for n in names]
files += [ROOT/'target/experiments/residual-stage-v1/0073/stages.json',ROOT/'docs/experiments/render-acceleration-v2/manifest.json']
files += [ROOT/f'target/experiments/render-acceleration-v1/compare_{n}.mp4.json' for n in ['late','control','additional']]
manifest={}
for p in files:
    h=hashlib.sha256()
    with p.open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    manifest[str(p.relative_to(ROOT))]=h.hexdigest()
(DOC/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print('Validated',len(videos),'videos, gate support and source margins.')
print(json.dumps(summary['selected_burst_changes']))
print(json.dumps(summary['windows']))
