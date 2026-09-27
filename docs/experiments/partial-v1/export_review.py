"""partial-v1 review exports: LEFT pv1prod (today's gate + floor), RIGHT pv1local (sliding-window
weight) on the two over-cap rejected bursts where the weight made a real render difference."""
import json, subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
RV2 = ROOT / 'target/experiments/regions-v2'
FF, FP = 'C:/ffmpeg/bin/ffmpeg.exe', 'C:/ffmpeg/bin/ffprobe.exe'
WINDOWS = [('0076nr', 'partial_290', 288.5, 4.5), ('0058nr', 'partial_345', 343.5, 4.5)]


def frames(p):
    return int(subprocess.check_output([FP, '-v', 'error', '-select_streams', 'v:0', '-count_packets',
        '-show_entries', 'stream=nb_read_packets', '-of', 'csv=p=0', str(p)], text=True).strip())


rows = []
for clip, label, start, dur in WINDOWS:
    left, right = RV2 / clip / 'pv1prod-render.mp4', RV2 / clip / 'pv1local-render.mp4'
    for suffix, panel in [('', 'scale=720:405,pad=720:406:0:0'), ('_full', 'null'), ('_detail', 'crop=720:405:720:0')]:
        dest = RV2 / clip / f'compare_{label}{suffix}.mp4'
        if not dest.exists():
            subprocess.run([FF, '-v', 'error', '-n', '-ss', str(start), '-i', str(left), '-ss', str(start), '-i', str(right),
                '-t', str(dur), '-filter_complex', f'[0:v]{panel}[a];[1:v]{panel}[b];[a][b]hstack[v]',
                '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '16', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)], check=True)
        n = frames(dest)
        assert n == round(dur * 100), (dest, n)
        rows.append({'video': str(dest.relative_to(RV2)), 'start': start, 'duration': dur, 'frames': n})
print(json.dumps(rows, indent=1))
