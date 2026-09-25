"""Side-by-side review exports: LEFT accepted edgeoffset render, RIGHT fb1w feedback render (world-frame carry)."""
import json, subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'target/experiments/feedback-v1'
LEFT = ROOT / 'target/experiments/generalization-v1/0073/edgeoffset-render.mp4'
RIGHT = OUT / 'fb1w-render.mp4'
FF, FP = 'C:/ffmpeg/bin/ffmpeg.exe', 'C:/ffmpeg/bin/ffprobe.exe'
WINDOWS = [('early', 14, 16), ('late', 132, 14), ('control', 55.9, 3)]


def frames(p):
    return int(subprocess.check_output([FP, '-v', 'error', '-select_streams', 'v:0', '-count_packets',
        '-show_entries', 'stream=nb_read_packets', '-of', 'csv=p=0', str(p)], text=True).strip())


rows = []
for label, start, dur in WINDOWS:
    for suffix, panel in [('', 'scale=720:405,pad=720:406:0:0'), ('_full', 'null'), ('_detail', 'crop=720:405:720:0')]:
        dest = OUT / f'compare_{label}{suffix}.mp4'
        if not dest.exists():
            subprocess.run([FF, '-v', 'error', '-n', '-ss', str(start), '-i', str(LEFT), '-ss', str(start), '-i', str(RIGHT),
                '-t', str(dur), '-filter_complex', f'[0:v]{panel}[a];[1:v]{panel}[b];[a][b]hstack[v]',
                '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '16', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)], check=True)
        n = frames(dest)
        assert n == round(dur * 100), (dest, n)
        rows.append({'video': dest.name, 'start': start, 'duration': dur, 'frames': n})
(OUT / 'review-validation.json').write_text(json.dumps(rows, indent=2))
print(json.dumps(rows, indent=1))
