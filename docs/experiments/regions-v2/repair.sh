export PATH="$PATH:/c/opencv/build/x64/vc16/bin:/c/Program Files/LLVM/bin"
cd /c/Users/lzhan/Desktop/o4prostab
for c in 20260919130352_0076 20260808145055_0057 20260808145758_0058 20260808151054_0059; do
  id=${c##*_}
  ./target/release/o4fix.exe sample_vids/DJI_${c}_D.MP4 -o target/experiments/regions-v2/cur_$id.MP4 > target/experiments/regions-v2/cur_${id}_run.log 2>&1
done
echo done
