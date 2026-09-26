"""Profile one o4fix run: timestamped log lines (stage timing) + 0.5 s samples of the process
tree (CPU cores used, RSS, threads) and system-wide CPU/memory.
  profile.py OUT_PREFIX -- o4fix.exe ARGS..."""
import sys, time, threading, subprocess, json
import psutil
pre = sys.argv[1]; cmd = sys.argv[sys.argv.index('--') + 1:]
t0 = time.time(); lines = []
p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
def reader():
    for ln in p.stdout:
        lines.append((round(time.time() - t0, 2), ln.rstrip()))
th = threading.Thread(target=reader, daemon=True); th.start()
proc = psutil.Process(p.pid); samples = []
psutil.cpu_percent(None); last = {}
while p.poll() is None:
    time.sleep(0.5)
    try:
        procs = [proc] + proc.children(recursive=True)
        cpu = rss = thr = 0.0
        for q in procs:
            with q.oneshot():
                ct = q.cpu_times(); c = ct.user + ct.system
                cpu += c - last.get(q.pid, c); last[q.pid] = c
                rss += q.memory_info().rss; thr += q.num_threads()
        samples.append(dict(t=round(time.time() - t0, 2), cores=round(cpu / 0.5, 2), rss_mb=round(rss / 2**20),
                            threads=int(thr), nproc=len(procs), sys_cpu=psutil.cpu_percent(None),
                            sys_mem=psutil.virtual_memory().percent))
    except psutil.NoSuchProcess:
        break
th.join(5)
json.dump(dict(cmd=cmd, wall=round(time.time() - t0, 1), rc=p.returncode, lines=lines, samples=samples), open(pre + '.json', 'w'), indent=0)
print('wall', round(time.time() - t0, 1), 'rc', p.returncode)
