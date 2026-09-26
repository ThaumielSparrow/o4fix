# no-refine repairs + label on every burst (sequential; renders one at a time)
export PATH="$PATH:/c/opencv/build/x64/vc16/bin:/c/Program Files/LLVM/bin"
cd /c/Users/lzhan/Desktop/o4prostab
G="python docs/experiments/regions-v2/gatecheck.py"
for c in 0076 0058 0057 0059 0027 0071 0073; do
  src=$(python -c "import sys;sys.path.insert(0,'docs/experiments/regions-v2');import gatecheck as g;print(g.SRC['$c'])")
  o=target/experiments/regions-v2/cur_${c}nr
  [ -f $o.MP4 ] || ./target/release/o4fix.exe --no-refine sample_vids/$src.MP4 -o $o.MP4 > ${o}_run.log 2>&1 || { echo "REPAIR FAIL $c"; continue; }
  { $G probe ${c}nr && $G render ${c}nr cur && $G measure ${c}nr cur && $G label ${c}nr; } > target/experiments/regions-v2/${c}nr_pipe.log 2>&1 || { echo "PIPE FAIL $c"; tail -3 target/experiments/regions-v2/${c}nr_pipe.log; continue; }
  echo "LABELED $c $(grep -c ' pred ' target/experiments/regions-v2/${c}nr_pipe.log) bursts, admits $(grep -c ADMIT target/experiments/regions-v2/${c}nr_pipe.log)"
done
echo ALL DONE
