"""Validate full context renders, then export accepted-left / partner-gate-right."""
import json
import pathlib
import subprocess

out=pathlib.Path('target/experiments/partner-gate-v1')
base=pathlib.Path('target/experiments/generalization-v1/0073/edgeoffset-render.mp4')
candidate=out/'partnergate-render.mp4'
ffmpeg='C:/ffmpeg/bin/ffmpeg.exe'
ffprobe='C:/ffmpeg/bin/ffprobe.exe'

def probe(path):
    return json.loads(subprocess.check_output([ffprobe,'-v','error','-select_streams','v:0',
        '-show_entries','stream=width,height,nb_frames,r_frame_rate,duration','-of','json',str(path)],text=True))['streams'][0]

full={}
for p in [base,candidate]:
    info=probe(p)
    assert (info['width'],info['height'],info['nb_frames'],info['r_frame_rate'])==(1440,810,'37594','100/1'),info
    full[str(p)]=info
reviews=[]
for label,start,duration in [('early',14,16),('late',132,14),('control',55.9,3),('additional',264,8)]:
    dest=out/f'compare_{label}.mp4'
    if not dest.exists():
        subprocess.run([ffmpeg,'-v','error','-n','-ss',str(start),'-i',str(base),'-ss',str(start),'-i',str(candidate),
            '-t',str(duration),'-filter_complex',
            '[0:v]scale=720:405,pad=720:406:0:0[a];[1:v]scale=720:405,pad=720:406:0:0[b];[a][b]hstack[v]',
            '-map','[v]','-an','-c:v','libx264','-crf','18','-preset','fast','-pix_fmt','yuv420p',str(dest)],check=True)
    info=probe(dest)
    assert (info['nb_frames'],info['r_frame_rate'])==(str(duration*100),'100/1'),info
    reviews.append(dict(label=label,start=start,duration=duration,video=info))
(out/'review-validation.json').write_text(json.dumps(dict(full=full,reviews=reviews),indent=2))
if not (out/'review-frame.png').exists():
    subprocess.run([ffmpeg,'-v','error','-n','-ss','10.6','-i',str(out/'compare_late.mp4'),'-frames:v','1',str(out/'review-frame.png')],check=True)
