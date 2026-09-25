import hashlib,json,math,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
D=ROOT/'target/experiments/render-acceleration-v2'
DOC=Path(__file__).resolve().parent
summary={}
for name in ['late','control','additional']:
    entry={}
    for stage in ['input','prior','full']:
        data=json.loads((D/f'{name}-{stage}.json').read_text(encoding='utf-8'))
        rows=[r['score'] for r in data['rows'][50:-50] if r['score']]
        b=sum(r['region_before_ss'] for r in rows)
        entry[stage]={'frames':data['frames'],'valid_interior':len(rows),'missing_total':sum(r['score'] is None for r in data['rows']),
            'region_translation_energy_reduction':1-sum(r['region_after_ss'] for r in rows)/b,
            'region_rigid_energy_reduction':1-sum(r['rigid_region_after_ss'] for r in rows)/b,
            'shared_acceleration_rms':math.sqrt(sum(sum(v*v for v in r['shared']) for r in rows)/len(rows)),
            'all_feature_acceleration_rms':math.sqrt(sum(r['heldout_before_ss'] for r in rows)/sum(r['n'] for r in rows))}
    bands=json.loads((D/f'{name}-bands.json').read_text())
    entry['bands']=bands
    bins=[]
    raw=json.loads((D/f'{name}-prior.json').read_text())['rows'][50:-50]
    for index in sorted(set(int(r['t']*4) for r in raw)):
        rows=[r['score'] for r in raw if int(r['t']*4)==index and r['score']]
        denom=sum(r['region_before_ss'] for r in rows)
        if denom>0:
            bins.append({'start':index/4,'count':len(rows),'translation_transfer':1-sum(r['region_after_ss'] for r in rows)/denom,'rigid_transfer':1-sum(r['rigid_region_after_ss'] for r in rows)/denom})
    entry['prior_quarter_second_diagnostics']=bins
    entry['band_percent_changes']=[{k:100*(a[k]/b[k]-1) for k in ['translation_rms_half_px_s','roll_rms_deg_s']} for b,a in zip(bands['panels'][0]['bands'],bands['panels'][1]['bands'])]
    transforms=json.loads((D/f'compare_{name}_full.mp4.json').read_text())
    entry['max_correction_half_px']=transforms['max_px']
    # Inverse-warp source support, including Lanczos4 margin, across all corrected frames.
    margins=[]
    for c in transforms['corrections']:
        for k,(center,last) in enumerate([(720.,1439.),(405.,809.)]):
            lo=center+(0-center)/1.04-2*c[k]
            hi=center+(last-center)/1.04-2*c[k]
            margins.append(min(lo,last-hi))
    entry['minimum_source_margin_px']=min(margins)
    assert min(margins)>4
    entry['video_checks']={}
    for kind,width,height in [('full',2880,810),('preview',1440,406)]:
        f=D/(f'compare_{name}_full.mp4' if kind=='full' else f'compare_{name}.mp4')
        p=subprocess.run(['C:/ffmpeg/bin/ffprobe.exe','-v','error','-count_frames','-select_streams','v:0','-show_entries','stream=width,height,r_frame_rate,nb_read_frames','-of','json',str(f)],capture_output=True,text=True,check=True)
        video=json.loads(p.stdout)['streams'][0]
        assert (video['width'],video['height'],video['r_frame_rate'],int(video['nb_read_frames']))==(width,height,'100/1',entry['input']['frames'])
        entry['video_checks'][kind]=video
    summary[name]=entry
    print(name,json.dumps(entry['prior']),json.dumps(entry['full']),entry['band_percent_changes'])
(DOC/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
files=[ROOT/'o4core/examples'/f'{n}.rs' for n in ['render_acceleration_full_probe','render_acceleration_full_review','splice_render_measure','local_render_smooth']]
files+=list(D.glob('*.json'))+list(D.glob('*.mp4'))+list(D.glob('*.mkv'))
files+=[ROOT/'target/experiments/bounce-localize-v1/gap2edge-render.mp4']
files+=list(DOC.glob('*.py'))
files+=[ROOT/'target/release/examples'/f'{n}.exe' for n in ['render_acceleration_full_probe','render_acceleration_full_review','splice_render_measure']]
files+=[ROOT/'target/experiments/render-acceleration-v1'/f'compare_{n}.mp4.json' for n in ['late','control','additional']]
manifest={}
for p in files:
    h=hashlib.sha256()
    with p.open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    manifest[str(p.relative_to(ROOT))]=h.hexdigest()
(DOC/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
