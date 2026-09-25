"""Summarize actual decoded-image probes; preserve missing-support failures."""
import hashlib
import json
import math
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'target/experiments/render-acceleration-v1'
DOC = Path(__file__).resolve().parent

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

summary = {}
for name in ['late', 'control', 'early', 'additional']:
    data = read(OUT / f'{name}-regions.json')
    rows = [r['score'] for r in data['rows'] if r['score']]
    item = {'frames': data['frames'], 'valid_triplets': len(rows),
            'missing_times': [r['t'] for r in data['rows'] if not r['score']]}
    for key, pre, post in [('feature_heldout_energy_reduction', 'heldout_before_ss', 'heldout_after_ss'),
                           ('region_heldout_energy_reduction', 'region_before_ss', 'region_after_ss')]:
        item[key] = 1-sum(r[post] for r in rows)/sum(r[pre] for r in rows)
    if (OUT / f'{name}-after.json').exists():
        before = read(OUT / f'{name}-before.json')['rows']
        after = read(OUT / f'{name}-after.json')['rows']
        common = [(a['score'], b['score']) for a, b in zip(before[50:-50], after[50:-50])
                  if a['score'] and b['score']]
        item['common_triplets'] = len(common)
        item['encoded_shared_acceleration_rms'] = [math.sqrt(sum(sum(v*v for v in r[k]['shared']) for r in common)/len(common)) for k in range(2)]
        item['encoded_all_feature_acceleration_rms'] = [math.sqrt(sum(r[k]['heldout_before_ss'] for r in common)/sum(r[k]['n'] for r in common)) for k in range(2)]
        item['bands'] = read(OUT / f'{name}-bands.json')
        item['max_correction_px'] = read(OUT / f'compare_{name}.mp4.json')['max_px']
        probe = subprocess.run(['C:/ffmpeg/bin/ffprobe.exe', '-v', 'error', '-count_frames',
                                '-select_streams', 'v:0', '-show_entries',
                                'stream=width,height,r_frame_rate,nb_read_frames', '-of', 'json',
                                str(OUT / f'compare_{name}.mp4')], check=True, capture_output=True, text=True)
        video = json.loads(probe.stdout)['streams'][0]
        assert (video['width'], video['height'], video['r_frame_rate'], int(video['nb_read_frames'])) == (1440, 406, '100/1', data['frames'])
        item['validated_video'] = video
    summary[name] = item
(DOC / 'summary.json').write_text(json.dumps(summary, indent=2)+'\n', encoding='utf-8')
files = [ROOT / 'o4core/examples' / f'{n}.rs' for n in ['render_acceleration_probe', 'render_acceleration_review', 'splice_render_measure', 'local_render_smooth']]
files += [ROOT / 'target/release/examples' / f'{n}.exe' for n in ['render_acceleration_probe', 'render_acceleration_review', 'splice_render_measure']]
files += sorted(OUT.glob('*'))
files += [ROOT / 'target/experiments/subset-ensemble-v1' / f'compare_{n}.mp4' for n in summary]
manifest = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
(DOC / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n', encoding='utf-8')
for name, item in summary.items():
    print(name, 'valid', item['valid_triplets'], 'region reduction', item['region_heldout_energy_reduction'])
    if 'bands' in item:
        b, a = item['bands']['panels']
        for left, right in zip(b['bands'], a['bands']):
            print(left['hz'], {k: 100*(right[k]/left[k]-1) for k in ['translation_rms_half_px_s', 'roll_rms_deg_s']})
