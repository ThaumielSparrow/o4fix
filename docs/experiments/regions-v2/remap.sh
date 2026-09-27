cd /c/Users/lzhan/Desktop/o4prostab
for c in 0071 0005 0083 0076 0058 0057 0059 0027 0073; do
  python docs/experiments/regions-v2/remap.py run $c > target/experiments/regions-v2/remap_$c.log 2>&1 && echo "OK $c" || { echo "FAIL $c"; tail -3 target/experiments/regions-v2/remap_$c.log; }
done
echo ALLDONE
