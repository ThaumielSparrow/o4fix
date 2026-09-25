"""Export validation comparisons after both full renders finish; run from repo root."""
import json
import pathlib
import subprocess
import sys

clip = '0073'
windows = {
    '0071': [('throttle', 11, 10), ('late', 252, 8), ('control', 74, 3)],
    '0073': [('early', 14, 16), ('late', 132, 14), ('control', 55.9, 3)],
}[clip]
out = pathlib.Path('target/experiments/bounce-localize-v1')
baseline = pathlib.Path('target/experiments/generalization-v1') / clip

ffmpeg = 'C:/ffmpeg/bin/ffmpeg.exe'
ffprobe = 'C:/ffmpeg/bin/ffprobe.exe'


def probe(path):
    return json.loads(subprocess.check_output([
        ffprobe, '-v', 'error', '-select_streams', 'v:0',
        '-show_entries', 'stream=width,height,nb_frames,r_frame_rate,duration',
        '-of', 'json', str(path)], text=True))['streams'][0]


source = json.loads((baseline / 'video-info.json').read_text())
for path in [baseline/'edgeoffset-render.mp4',out/'gap2edge-render.mp4']:
    info=probe(path)
    assert (info['width'],info['height'],info['nb_frames'],info['r_frame_rate'])==(1440,810,source['nb_frames'],'100/1'),info

reviews = []
for label, start, duration in windows:
    dest = out / f'compare_{label}.mp4'
    subprocess.run([
        ffmpeg, '-v', 'error', '-n', '-ss', str(start), '-i', str(baseline / 'edgeoffset-render.mp4'),
        '-ss', str(start), '-i', str(out / 'gap2edge-render.mp4'), '-t', str(duration),
        '-filter_complex', '[0:v]scale=720:405,pad=720:406:0:0[a];[1:v]scale=720:405,pad=720:406:0:0[b];[a][b]hstack[v]',
        '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '18', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)
    ], check=True)
    info = probe(dest)
    assert info['nb_frames'] == str(duration * 100), info
    reviews.append({'label': label, 'start': start, 'duration': duration, 'video': info})
(out / 'review-validation.json').write_text(json.dumps(reviews, indent=2))
