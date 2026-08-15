import json
import sys
from pathlib import Path

import cv2

template, video_path, out_json, out_filename, out_folder = sys.argv[1:6]

with open(template, encoding="utf-8") as f:
    d = json.load(f)

video_uri = "file:///" + str(Path(video_path).resolve()).replace("\\", "/")
d["videofile"] = video_uri
d["gyro_source"]["filepath"] = video_uri
d["gyro_source"].pop("file_metadata", None)
d["offsets"] = {}

# video_info MUST match the retargeted clip. Gyroflow sizes its per-frame
# adaptive-zoom array from these fields (it does not re-probe the file when
# loading a project), so a template inherited from a shorter clip freezes the
# zoom at the template's last frame for the whole remainder of the render ->
# lens edges swing into frame. Cost the 0060/0027 A/B renders a re-do.
cap = cv2.VideoCapture(str(Path(video_path).resolve()))
if not cap.isOpened():
    raise SystemExit(f"cannot open {video_path}")
vi = d["video_info"]
vi["width"] = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
vi["height"] = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
vi["fps"] = cap.get(cv2.CAP_PROP_FPS)
vi["num_frames"] = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
vi["duration_ms"] = 1000.0 * vi["num_frames"] / vi["fps"]
vi["vfr_fps"] = vi["fps"]
vi["vfr_duration_ms"] = vi["duration_ms"]
cap.release()

d["output"]["output_filename"] = out_filename
folder_uri = "file:///" + str(Path(out_folder).resolve()).replace("\\", "/") + "/"
d["output"]["output_folder"] = folder_uri
d["output"]["bitrate"] = 30

with open(out_json, "w", encoding="utf-8") as f:
    json.dump(d, f)

print(f"wrote {out_json}")
print(f"  videofile = {video_uri}")
print(f"  video_info = {vi['width']}x{vi['height']} @{vi['fps']}, "
      f"{vi['num_frames']} frames, {vi['duration_ms'] / 1000:.1f} s")
print(f"  output_folder = {folder_uri}")
print(f"  output_filename = {out_filename}")
