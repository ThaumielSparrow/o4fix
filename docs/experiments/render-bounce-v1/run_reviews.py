import os,sys,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
os.chdir(ROOT)
os.environ['PATH']+=';C:/opencv/build/x64/vc16/bin'
D=Path('target/experiments/render-bounce-v1')
for name in sys.argv[1:]:
    start={'late':132,'control':55.9,'additional':264}[name]
    full=D/f'compare_{name}_full.mp4'
    if not full.exists():
        subprocess.run(['target/release/examples/render_bounce_review.exe',f'target/experiments/render-acceleration-v2/{name}.mkv',f'target/experiments/render-rotation-v1/{name}-probe.json',str(full),f'target/experiments/render-acceleration-v1/compare_{name}.mp4.json'],check=True)
    preview=D/f'compare_{name}.mp4'
    if not preview.exists():
        subprocess.run(['C:/ffmpeg/bin/ffmpeg.exe','-v','error','-n','-i',str(full),'-vf','scale=1440:405:flags=lanczos,pad=1440:406','-an','-c:v','libx264','-preset','fast','-crf','17',str(preview)],check=True)
    bands=D/f'{name}-bands.json'
    if not bands.exists():
        subprocess.run(['target/release/examples/splice_render_measure.exe',str(preview),str(start),'target/experiments/render-acceleration-v1/whole-window.json',str(bands)],check=True)
