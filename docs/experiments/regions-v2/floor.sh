# step 2: production-like refinement of every under-cap burst vs skipping agreement < 0.3
set -e
cd /c/Users/lzhan/Desktop/o4prostab
G="python docs/experiments/regions-v2/gatecheck.py"
for c in 0058nr 0057nr 0073nr; do
  for v in refined floor; do
    [ -f target/experiments/regions-v2/$c/$v-adjust.json ] || $G adjust $c $v $v | tail -1
    [ -f target/experiments/regions-v2/$c/$v.MP4 ] || $G repair $c $v > /dev/null
    $G render $c $v | grep frames; $G measure $c $v
  done
  $G score $c cur refined floor > target/experiments/regions-v2/$c/floor-score.txt
  echo "FLOOR DONE $c"
done
