"""Freeze render settings from the previously reviewed gap2-plus-edge candidate."""
import json
from pathlib import Path
out=Path('target/experiments/subset-ensemble-v1').resolve()
p=json.loads(Path('target/experiments/bounce-localize-v1/gap2edge.gyroflow').read_text())
assert p['video_info']['num_frames']==37594 and p['video_info']['fps']==100
assert p['offsets']=={} and p['stabilization']['horizon_lock_amount']==0
assert not p['synchronization']['auto_sync_points']
p['videofile']=(out/'ensemble.MP4').as_uri()
p['gyro_source']['filepath']=p['videofile'];p['gyro_source'].pop('file_metadata',None)
p['output']['output_filename']='ensemble-render.mp4';p['output']['output_folder']=out.as_uri()+'/'
with (out/'ensemble.gyroflow').open('x') as f:json.dump(p,f,indent=2)
with (out/'windows.json').open('x') as f:json.dump([[14,30],[132,146],[55.9,58.9],[264,272]],f)
