#!/usr/bin/env bash
# Gyroflow-free wp1c pass on a clip: probe base windows (render crop), correct, repair+render, score, reviews.
set -e
C=$1; B=$2
cd "$(dirname "$0")/../../.."
export PATH="$PATH:/c/opencv/build/x64/vc16/bin"
D=target/experiments/feedback-v1; W=$D/warp/$C; mkdir -p $W
F="python docs/experiments/feedback-v1/fbclip.py"
python - "$C" <<'PY' | tr -d '\015' > $W/windows.txt
import sys; sys.path.insert(0,'docs/experiments/feedback-v1'); import fbclip as F
for w,(a,b) in F.CLIPS[sys.argv[1]]['windows'].items(): print(w,a,b-a)
PY
while read w a d; do
  [ -f $W/basecrop-$w.json ] || O4_CROP=1.30,0.72 target/release/examples/warp_residual_probe.exe $B $a $d 5.092569 0 3 $W/basecrop-$w.json
done < $W/windows.txt
python -c "import sys; sys.path.insert(0,'docs/experiments/feedback-v1'); import warpcmp as W; W.correct_from_warp('$C','wp1c',prefix='basecrop')"
$F repair $C wp1c $B
$F measure $C wp1c $D/$C/wp1c-render.mp4
$F score $C base:$D/$C/base-camera.json fbn:$D/$C/fbn-camera.json wp1c:$D/$C/wp1c-camera.json
$F reviews_pair $C fbn wp1c
echo DONE $C
