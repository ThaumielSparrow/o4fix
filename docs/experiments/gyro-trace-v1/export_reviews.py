"""Run from the repository root after ramp019-render.mp4 finishes."""
import sys
import json
import pathlib
import subprocess

root = pathlib.Path.cwd()
out = root / 'target/experiments/gyro-trace-v1'
variant = sys.argv[1]
assert variant in ['pad040', 'edgeoffset']
baseline = root / 'target/experiments/splice-exposure-v1/ramp019-render.mp4'
candidate = out / f'{variant}-render.mp4'
ffmpeg = 'C:/ffmpeg/bin/ffmpeg.exe'
ffprobe = 'C:/ffmpeg/bin/ffprobe.exe'


def probe(path):
    return json.loads(subprocess.check_output([
        ffprobe, '-v', 'error', '-select_streams', 'v:0',
        '-show_entries', 'stream=width,height,nb_frames,r_frame_rate,duration',
        '-of', 'json', str(path)], text=True))['streams'][0]


for path in [baseline, candidate]:
    info = probe(path)
    assert (info['width'], info['height'], info['nb_frames'], info['r_frame_rate']) == (1440, 810, '38269', '100/1'), info

reports = []
for label, start, duration in [('106', 102, 8), ('225', 221, 8), ('249', 245, 8), ('308', 304, 8), ('clean', 19, 3)]:
    dest = out / f'{variant}_compare_{label}.mp4'
    subprocess.run([
        ffmpeg, '-v', 'error', '-n', '-ss', str(start), '-i', str(baseline),
        '-ss', str(start), '-i', str(candidate), '-t', str(duration),
        '-filter_complex', '[0:v]scale=720:405,pad=720:406:0:0[a];[1:v]scale=720:405,pad=720:406:0:0[b];[a][b]hstack[v]',
        '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '18', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)
    ], check=True)
    info = probe(dest)
    assert info['nb_frames'] == str(duration * 100), info
    reports.append({'label': label, 'start': start, 'duration': duration, 'video': info})
(out / f'{variant}-review-validation.json').write_text(json.dumps(reports, indent=2))
