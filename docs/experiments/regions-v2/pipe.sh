# per-clip pipeline: label on cur, then forced-all and gated variants rendered + scored
set -e
cd /c/Users/lzhan/Desktop/o4prostab
G="python docs/experiments/regions-v2/gatecheck.py"
c=$1
until [ -f target/experiments/regions-v2/cur_$c.MP4 ] && ! tasklist | grep -qi "o4fix.exe" || [ -f target/experiments/regions-v2/cur_${c}_done ]; do sleep 15; done
# ensure this clip's repair log is complete (writer finished)
grep -q "refining residual" target/experiments/regions-v2/cur_${c}_run.log
n=$(grep -c "over cap" target/experiments/regions-v2/cur_${c}_run.log || true)
echo "$c capped=$n"
[ "$n" = "0" ] && exit 0
[ -f target/experiments/regions-v2/$c/labels.json ] || { $G probe $c; $G render $c cur; $G measure $c cur; $G label $c; }
for v in all gate; do
  if [ $v = gate ] && ! python -c "import json,sys;sys.exit(0 if any(r['pred_free']>=0.75 for r in json.load(open('target/experiments/regions-v2/$c/labels.json'))) else 1)"; then echo "$c no admits"; continue; fi
  [ -f target/experiments/regions-v2/$c/$v-adjust.json ] || $G adjust $c $v $v
  [ -f target/experiments/regions-v2/$c/$v.MP4 ] || $G repair $c $v
  $G render $c $v; $G measure $c $v
done
if [ -f target/experiments/regions-v2/$c/gate.MP4 ]; then $G score $c cur all gate; else $G score $c cur all; fi
echo "PIPE DONE $c"
