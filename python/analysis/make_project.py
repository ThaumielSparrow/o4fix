import json
import sys
from pathlib import Path

template, video_path, out_json, out_filename, out_folder = sys.argv[1:6]

with open(template, encoding="utf-8") as f:
    d = json.load(f)

video_uri = "file:///" + str(Path(video_path).resolve()).replace("\\", "/")
d["videofile"] = video_uri
d["gyro_source"]["filepath"] = video_uri
d["gyro_source"].pop("file_metadata", None)
d["offsets"] = {}
d["output"]["output_filename"] = out_filename
folder_uri = "file:///" + str(Path(out_folder).resolve()).replace("\\", "/") + "/"
d["output"]["output_folder"] = folder_uri
d["output"]["bitrate"] = 30

with open(out_json, "w", encoding="utf-8") as f:
    json.dump(d, f)

print(f"wrote {out_json}")
print(f"  videofile = {video_uri}")
print(f"  output_folder = {folder_uri}")
print(f"  output_filename = {out_filename}")
