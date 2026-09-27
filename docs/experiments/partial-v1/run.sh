# partial-v1 step 2: production's refine set (pv1prod) vs every burst weighted by local0.4 (pv1local)
set -e
cd /c/Users/lzhan/Desktop/o4prostab
G="python docs/experiments/regions-v2/gatecheck.py"
for c in 0058nr 0076nr 0073nr; do
  [ -f target/experiments/regions-v2/$c/pv1local-adjust.json ] || python docs/experiments/partial-v1/drive.py adjust $c
  for v in pv1prod pv1local; do
    [ -f target/experiments/regions-v2/$c/$v.MP4 ] || $G repair $c $v > /dev/null
    $G render $c $v | grep frames; $G measure $c $v
  done
  $G score $c cur pv1prod pv1local > target/experiments/regions-v2/$c/pv1-score.txt
  echo "PV1 DONE $c"
done
python docs/experiments/partial-v1/compare.py
