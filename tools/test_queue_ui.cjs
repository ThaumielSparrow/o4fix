// Run with node tools/test_queue_ui.cjs; no browser or npm dependencies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

// Minimal stand-in for the DOM app.js touches: any element, any child
// looked up by selector, created on first use.
function fakeEl() {
  const kids = {};
  return {
    textContent: '', hidden: false, disabled: false, className: '', title: '',
    innerHTML: '', dataset: {}, children: [], parent: null,
    style: { setProperty() {} },
    classList: { add() {}, remove() {}, toggle() {}, contains: () => false },
    querySelector(s) { return (kids[s] ??= fakeEl()); },
    querySelectorAll: () => [],
    appendChild(c) { this.children.push(c); c.parent = this; return c; },
    remove() { if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1); },
    setAttribute() {}, focus() {},
  };
}

function harness() {
  let resolve, reject, calls = 0;
  const elements = {};
  const context = vm.createContext({
    window: {
      __TAURI__: {
        core: { invoke: () => { calls++; return new Promise((a, b) => { resolve = a; reject = b; }); } },
        event: { listen: () => {} },
      },
      o4trace: { hero() {}, ClipTrace: class { setBefore() {} setAfter() {} } },
    },
    document: {
      getElementById: k => (elements[k] ??= fakeEl()),
      createElement: () => fakeEl(),
      querySelector: () => fakeEl(),
      querySelectorAll: () => [],
      addEventListener() {},
    },
  });
  const source = fs.readFileSync(path.join(__dirname, '../o4fix-app/ui/app.js'), 'utf8');
  vm.runInContext(source.replace(/init\(\);\s*$/, ''), context);
  const run = s => vm.runInContext(s, context);
  run('addFiles(["clip.MP4"]); settings = {};');
  return { run, elements, resolve: x => resolve(x), reject: x => reject(x), calls: () => calls };
}

(async () => {
  const h = harness();
  const first = h.run('start()');
  assert.equal(h.elements.start.disabled, true);
  await h.run('start()');
  h.run('addFiles(["other.MP4"])');
  assert.equal(h.calls(), 1, 'only one queue may start during IPC');
  assert.equal(h.run('pending.length'), 1, 'no orphaned row during startup');
  h.run('onDone({payload:{id:7,status:"healthy",message:"healthy"}})');
  h.resolve([7]);
  await first;
  assert.equal(h.run('activeJobs'), 0, 'early completion replays exactly once');
  assert.equal(h.run('pending.length'), 0);
  assert.equal(h.run('busy()'), false);

  const failed = harness();
  const attempt = failed.run('start()');
  failed.reject('IPC failure');
  await attempt;
  assert.equal(failed.run('pending.length'), 1, 'failed startup keeps retryable inputs');
  assert.equal(failed.elements.start.disabled, false);
  assert.match(failed.run('pending[0].clip.msg.textContent'), /IPC failure/,
    'failed startup is reported on the clip row');

  // a retry that succeeds clears the stale failure message
  const retry = failed.run('start()');
  failed.resolve([3]);
  await retry;
  assert.equal(failed.run('rows.get(3).msg.textContent'), '', 'retry clears the old error');
  console.log('Queue startup race and failure recovery: PASS');
})().catch(e => { console.error(e); process.exitCode = 1; });
