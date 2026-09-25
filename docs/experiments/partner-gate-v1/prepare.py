"""Create a fresh full-clip project from the accepted 0073 settings."""
import json
from pathlib import Path

out = Path('target/experiments/partner-gate-v1').resolve()
base = Path('target/experiments/generalization-v1/0073/edgeoffset.gyroflow')
project = json.loads(base.read_text())
assert project['video_info']['num_frames'] == 37594
assert project['stabilization']['horizon_lock_amount'] == 0
assert project['offsets'] == {}
assert project['synchronization']['auto_sync_points'] is False
project['videofile'] = (out/'partnergate.MP4').as_uri()
project['gyro_source']['filepath'] = project['videofile']
project['gyro_source'].pop('file_metadata', None)
project['output']['output_filename'] = 'partnergate-render.mp4'
project['output']['output_folder'] = out.as_uri()+'/'
with (out/'partnergate.gyroflow').open('x') as f:
    json.dump(project, f, indent=2)
with (out/'windows.json').open('x') as f:
    json.dump([[14,30],[132,146],[55.9,58.9],[142.5,142.75],[143,143.25],[143.25,143.5],[143.5,143.75]],f)
