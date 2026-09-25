import os, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
os.chdir(ROOT)
os.environ['PATH'] += ';C:/opencv/build/x64/vc16/bin'
D=Path('target/experiments/render-acceleration-v2')
for name,start in [('late',132),('control',55.9),('additional',264)]:
    out=D/f'compare_{name}_full.mp4'
    if not out.exists():
        subprocess.run(['target/release/examples/render_acceleration_full_review.exe',str(D/f'{name}.mkv'),str(D/f'{name}-input.json'),str(out),f'target/experiments/render-acceleration-v1/compare_{name}.mp4.json'],check=True)
    for panel,label in [(0,'prior'),(1,'full')]:
        result=D/f'{name}-{label}.json'
        if not result.exists():
            subprocess.run(['target/release/examples/render_acceleration_full_probe.exe',str(out),str(start),str(panel),str(result)],check=True)
    preview=D/f'compare_{name}.mp4'
    if not preview.exists():
        subprocess.run(['C:/ffmpeg/bin/ffmpeg.exe','-v','error','-n','-i',str(out),'-vf','scale=1440:405:flags=lanczos,pad=1440:406','-an','-c:v','libx264','-crf','17','-preset','fast',str(preview)],check=True)
    bands=D/f'{name}-bands.json'
    if not bands.exists():
        subprocess.run(['target/release/examples/splice_render_measure.exe',str(preview),str(start),'target/experiments/render-acceleration-v1/whole-window.json',str(bands)],check=True)
