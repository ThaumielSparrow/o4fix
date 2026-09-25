"""Full-context validation then gap2-edge left / subset ensemble right comparisons."""
import json
from pathlib import Path
import subprocess
out=Path('target/experiments/subset-ensemble-v1')
base=Path('target/experiments/bounce-localize-v1/gap2edge-render.mp4')
candidate=out/'ensemble-render.mp4'
ffmpeg='C:/ffmpeg/bin/ffmpeg.exe'; ffprobe='C:/ffmpeg/bin/ffprobe.exe'
def probe(p):
    return json.loads(subprocess.check_output([ffprobe,'-v','error','-select_streams','v:0','-show_entries','stream=width,height,nb_frames,r_frame_rate,duration','-of','json',str(p)],text=True))['streams'][0]
full={}
for p in [base,candidate]:
    d=probe(p);assert (d['width'],d['height'],d['nb_frames'],d['r_frame_rate'])==(1440,810,'37594','100/1'),d
    full[str(p)]=d
reviews=[]
for label,start,duration in [('early',14,16),('late',132,14),('control',55.9,3),('additional',264,8)]:
    dest=out/f'compare_{label}.mp4'
    if not dest.exists():
        subprocess.run([ffmpeg,'-v','error','-n','-ss',str(start),'-i',str(base),'-ss',str(start),'-i',str(candidate),'-t',str(duration),
            '-filter_complex','[0:v]scale=720:405,pad=720:406:0:0[a];[1:v]scale=720:405,pad=720:406:0:0[b];[a][b]hstack[v]',
            '-map','[v]','-an','-c:v','libx264','-crf','18','-preset','fast','-pix_fmt','yuv420p',str(dest)],check=True)
    d=probe(dest);assert d['nb_frames']==str(duration*100) and d['r_frame_rate']=='100/1',d
    reviews.append(dict(label=label,start=start,duration=duration,video=d))
(out/'review-validation.json').write_text(json.dumps(dict(full=full,reviews=reviews),indent=2))
if not (out/'review-frame.png').exists():
    subprocess.run([ffmpeg,'-v','error','-n','-ss','10.6','-i',str(out/'compare_late.mp4'),'-frames:v','1',str(out/'review-frame.png')],check=True)
