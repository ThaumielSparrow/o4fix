// Run with node tools/test_trace_warp.cjs; no browser or npm dependencies.
// Exercises o4trace.buildWarp (trace.js), the pure progress-space ->
// clip-time warp used by the per-clip trace's progress-follow playhead.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

function load() {
  // trace.js only touches the DOM inside functions we won't call here
  // (el(), reduceMotion()); buildWarp() itself is pure.
  const context = vm.createContext({
    window: {},
    document: { createElementNS() { return {}; } },
    matchMedia: () => ({ matches: false }),
  });
  const source = fs.readFileSync(path.join(__dirname, '../o4fix-app/ui/trace.js'), 'utf8');
  vm.runInContext(source, context);
  return context.window.o4trace.buildWarp;
}

const buildWarp = load();

// no bursts at all: uniform mapping, indexAt is just p*n (straight sweep)
{
  const w = buildWarp(100, [], {});
  assert.equal(w.indexAt(0), 0);
  assert.equal(w.indexAt(1), 100);
  for (const p of [0.1, 0.37, 0.5, 0.82]) {
    assert.ok(Math.abs(w.indexAt(p) - p * 100) < 1e-9, `uniform map at p=${p}`);
  }
  console.log('no-burst uniform mapping: PASS');
}

// progressAt is monotonically non-decreasing and spans exactly [0,1]
{
  const w = buildWarp(50, [[10, 15], [40, 41]], { calmWeight: 1, burstWeight: 10, minBurstWeight: 0 });
  assert.equal(w.progressAt[0], 0);
  assert.equal(w.progressAt[w.n], 1);
  for (let i = 1; i <= w.n; i++) assert.ok(w.progressAt[i] >= w.progressAt[i - 1], `monotonic at ${i}`);
  console.log('monotonic progressAt: PASS');
}

// a burst consumes much more progress per bucket than calm flight, so for an
// equal-size span the burst's progress share is far larger than calm's
{
  const n = 100;
  const burst = [40, 49]; // 10 buckets
  const w = buildWarp(n, [burst], { calmWeight: 1, burstWeight: 10, minBurstWeight: 0 });
  const shareOf = ([a, b]) => w.progressAt[b + 1] - w.progressAt[a];
  const burstShare = shareOf(burst);
  const calmShare = shareOf([0, 9]); // same width, pure calm
  assert.ok(burstShare > calmShare * 5, `burst (${burstShare}) should dominate calm (${calmShare})`);
  console.log('burst gets disproportionate progress weight: PASS');
}

// a tiny (1-bucket) burst still gets a meaningfully larger share than a
// single calm bucket, thanks to the per-burst minimum weight
{
  const n = 200;
  const w = buildWarp(n, [[100, 100]], { calmWeight: 1, burstWeight: 10, minBurstWeight: 18 });
  const tinyBurstShare = w.progressAt[101] - w.progressAt[100];
  const calmBucketShare = w.progressAt[1] - w.progressAt[0];
  assert.ok(tinyBurstShare > calmBucketShare * 10,
    `tiny burst (${tinyBurstShare}) should still be boosted well above one calm bucket (${calmBucketShare})`);
  console.log('minimum burst weight boosts tiny bursts: PASS');
}

// indexAt is the (approximate) inverse of progressAt at the bucket boundaries
{
  const w = buildWarp(64, [[5, 9], [50, 60]], { calmWeight: 1, burstWeight: 12, minBurstWeight: 20 });
  for (let i = 0; i <= w.n; i++) {
    const back = w.indexAt(w.progressAt[i]);
    assert.ok(Math.abs(back - i) < 1e-6, `indexAt inverts progressAt at bucket ${i} (got ${back})`);
  }
  console.log('indexAt inverts progressAt: PASS');
}

// degenerate n=0 never throws and stays at the start
{
  const w = buildWarp(0, [], {});
  assert.equal(w.indexAt(0.5), 0);
  console.log('degenerate n=0 handled: PASS');
}

console.log('All trace warp tests passed.');
