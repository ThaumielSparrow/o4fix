// Gyro-noise traces: the launch-screen sweep and the per-clip trace.
// Amber = burst noise still in the telemetry, teal = repaired.
(() => {
const NS = "http://www.w3.org/2000/svg";
const reduceMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const ease = (x) => (x <= 0 ? 0 : x >= 1 ? 1 : 1 - Math.pow(1 - x, 3));
const mix = (p) => `color-mix(in srgb, var(--fixed) ${Math.round(p * 100)}%, var(--noise))`;

function el(tag, attrs, parent) {
  const e = document.createElementNS(NS, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (parent) parent.appendChild(e);
  return e;
}

// Runs step(t) every frame until it returns false; reduced motion jumps to
// the end state. Returns a cancel function.
// Frames pause while the window is minimized/hidden, so a timer also lands
// the end state in case they never run.
function animate(step, dur) {
  if (reduceMotion()) { step(dur); return () => {}; }
  let t0 = null, raf = 0, done = false;
  const frame = (now) => {
    if (done) return;
    if (t0 === null) t0 = now;
    const t = (now - t0) / 1000;
    step(Math.min(t, dur));
    if (t < dur) raf = requestAnimationFrame(frame); else done = true;
  };
  raf = requestAnimationFrame(frame);
  const timer = setTimeout(() => { if (!done) { done = true; step(dur); } }, dur * 1000 + 250);
  return () => { done = true; cancelAnimationFrame(raf); clearTimeout(timer); };
}

// ---------- launch screen: an illustrative flight, swept clean once ----------
function hero(svg) {
  const W = 700, H = 130, N = 350, MID = 65;
  const bursts = [[0.08, 0.15], [0.3, 0.47], [0.62, 0.67], [0.78, 0.92]];
  let s = 7;
  const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647) - 0.5;
  const base = [], noise = [], owner = [];
  for (let i = 0; i <= N; i++) {
    const u = i / N;
    base.push(MID + 20 * Math.sin(u * 9.1) + 11 * Math.sin(u * 23 + 1) + 5 * Math.sin(u * 51));
    let k = -1, env = 0;
    bursts.forEach(([a, b], j) => {
      if (u > a && u < b) { k = j; env = Math.min(1, Math.min(u - a, b - u) * 70); }
    });
    owner.push(k);
    noise.push(k >= 0 ? rnd() * 55 * env : 0); // calm stretches stay smooth
  }
  const segs = [];
  for (let i = 1, st = 0; i <= N; i++) {
    if (i === N || owner[i] !== owner[st]) { segs.push({ i0: st, i1: i, burst: owner[st] }); st = i; }
  }
  const x = (i) => (i / N) * W;
  const y = (v) => Math.max(3, Math.min(H - 3, v));

  svg.innerHTML = "";
  el("line", { class: "axis", x1: 0, y1: MID, x2: W, y2: MID }, svg);
  const paths = segs.map((sg) => {
    const p = el("path", {}, svg);
    if (sg.burst < 0) p.style.stroke = "var(--ink-3)";
    return p;
  });
  const head = el("line", { class: "head", y1: 4, y2: H - 4 }, svg);

  const T = 2.4, delay = 0.4, dur = T + delay + 0.6;
  const headX = (t) => Math.min(1, Math.max(0, (t - delay) / T)) * W;
  return animate((t) => {
    const hx = headX(t);
    const k = (i) => 1 - ease((hx - x(i)) / 60); // noise left at sample i
    segs.forEach((sg, j) => {
      let d = "";
      for (let i = sg.i0; i <= sg.i1; i++)
        d += (i === sg.i0 ? "M" : "L") + x(i).toFixed(1) + "," + y(base[i] + noise[i] * k(i)).toFixed(1);
      paths[j].setAttribute("d", d);
      if (sg.burst >= 0) paths[j].style.stroke = mix(1 - k((sg.i0 + sg.i1) >> 1));
    });
    head.setAttribute("x1", hx);
    head.setAttribute("x2", hx);
    head.style.opacity = t > dur - 0.4 ? Math.max(0, (dur - t) / 0.4) : t < delay ? 0 : 1;
  }, dur);
}

// ---------- per-clip trace: the detector's 30-180 Hz noise level ----------
const VW = 1000, VH = 44, FLOOR = 1.5; // deg/s: calm-flight floor (noise_low)
const SQUASH_WIDTH = 80;     // px: ease width a passing head squashes a sample over
const PROGRESS_CAP = 0.985;  // the head never shows "done" until real after-data lands
const CREEP_EPS = 0.01;      // how far displayed progress may lead the latest report
const CREEP_RATE = 0.05;     // /s: max self-advance while waiting for the next report
const FOLLOW_TAU = 0.2;      // s: exponential-follow time constant toward the creep target
const FINISH_DUR = 0.4;      // s: quick sweep to the end once the real fix lands
const SETTLE_DUR = 0.3;      // s: morph from the provisional (calm) squash to true values
const FADE_DUR = 0.3;        // s: head fade-out
const WARP = { calmWeight: 1, burstWeight: 10, minBurstWeight: 18 };

// Maps overall job progress p in [0,1] to a fractional bucket index along the
// clip, so a playhead driven by real progress zips through calm flight and
// slows through each noise burst. `ranges` are the same [i0,i1] bucket index
// pairs setBefore already computes for the bursts. Weight = progress "spent"
// per bucket: bursts get burstWeight per bucket (with a floor so even a
// 1-bucket burst still visibly slows the head), calm buckets get calmWeight.
// Pure function of its inputs; exposed on window.o4trace for testing.
function buildWarp(n, ranges, opts) {
  opts = opts || {};
  if (!(n > 0)) return { n: 0, progressAt: new Float64Array([0, 1]), indexAt: () => 0 };
  const calmW = opts.calmWeight ?? 1;
  const burstW = opts.burstWeight ?? 10;
  const minW = opts.minBurstWeight ?? 0;
  const w = new Float64Array(n).fill(calmW);
  for (const [i0, i1] of ranges) {
    const lo = Math.max(0, i0), hi = Math.min(n - 1, i1);
    const per = Math.max(burstW, minW / Math.max(1, hi - lo + 1));
    for (let i = lo; i <= hi; i++) w[i] = per;
  }
  const progressAt = new Float64Array(n + 1);
  for (let i = 0; i < n; i++) progressAt[i + 1] = progressAt[i] + w[i];
  const total = progressAt[n] || 1;
  for (let i = 0; i <= n; i++) progressAt[i] /= total;
  function indexAt(p) {
    p = p <= 0 ? 0 : p >= 1 ? 1 : p;
    let lo = 0, hi = n;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (progressAt[mid] < p) lo = mid + 1; else hi = mid;
    }
    const hiI = Math.max(1, Math.min(n, lo));
    const loI = hiI - 1;
    const span = progressAt[hiI] - progressAt[loI];
    return loI + (span > 0 ? (p - progressAt[loI]) / span : 0);
  }
  return { n, progressAt, indexAt };
}

class ClipTrace {
  constructor(host) {
    this.host = host;
    this.svg = el("svg", { viewBox: `0 0 ${VW} ${VH}`, preserveAspectRatio: "none", "aria-hidden": "true" }, host);
    this.before = null;
    this.phase = "idle"; // idle -> progress|static -> finish -> settle -> fadeAfter|fadeAbort -> done
    this.cancel = () => {};
  }

  // bucket index -> x, and the bucket range each burst covers
  x(i) { return ((i + 0.5) / this.n) * VW; }
  y(v) {
    const h = Math.sqrt(Math.max(0, v - FLOOR) / (this.top - FLOOR));
    return (VH - 3 - (VH - 8) * Math.min(1, h)).toFixed(1);
  }
  range([a, b]) {
    const f = (t) => ((t - this.t0) / (this.t1 - this.t0)) * this.n;
    return [Math.max(0, Math.floor(f(a))), Math.min(this.n - 1, Math.ceil(f(b)))];
  }

  setBefore({ t0, t1, values, bursts }) {
    Object.assign(this, { t0, t1, n: values.length });
    this.top = Math.max(30, ...values);
    this.before = values;
    this.inBurst = new Array(this.n).fill(-1);
    this.ranges = bursts.map((b, j) => {
      const r = this.range(b);
      for (let i = r[0]; i <= r[1]; i++) this.inBurst[i] = j;
      return r;
    });
    // calm stretches are drawn lightly smoothed: the eye should go to the bursts
    this.calm = values.map((v, i) => {
      let s = 0, c = 0;
      for (let k = Math.max(0, i - 3); k <= Math.min(this.n - 1, i + 3); k++)
        if (this.inBurst[k] < 0) { s += values[k]; c++; }
      return c ? s / c : v;
    });

    this.svg.innerHTML = "";
    this.bands = bursts.map(([a, b]) => {
      const x0 = ((a - t0) / (t1 - t0)) * VW, x1 = ((b - t0) / (t1 - t0)) * VW;
      const r = el("rect", { class: "band", x: x0, y: 0, width: Math.max(2, x1 - x0), height: VH }, this.svg);
      el("title", {}, r).textContent = `${a.toFixed(1)}–${b.toFixed(1)} s`;
      return r;
    });
    this.base = el("path", { class: "calm" }, this.svg);
    this.lines = bursts.map(() => el("path", { class: "noisy" }, this.svg));
    this.head = el("line", { class: "head", x1: 0, x2: 0, y1: 2, y2: VH - 2 }, this.svg);
    this.head.style.opacity = 0;
    this.draw(this.before, () => 0);
    this.host.classList.add("has-data");

    // ---- progress-follow playhead: real pct drives the head, warped so it
    // zips through calm flight and slows through bursts (see buildWarp) ----
    this.cancel();
    this.warp = buildWarp(this.n, this.ranges, WARP);
    this.pct = 0; this.disp = 0; this._creepTo = 0; this._headX = 0;
    this._last = 0; this._afterVals = null;
    if (reduceMotion() || this.n === 0) {
      // reduced motion: no animated head/squash preview; jump straight to the
      // final picture when setAfter/stop land (matches the old behavior).
      this.phase = "static";
      this.cancel = () => {};
    } else {
      this.phase = "progress";
      this.head.style.opacity = 1;
      this._raf = requestAnimationFrame((t) => this._tick(t));
      this.cancel = () => {
        if (this._raf) cancelAnimationFrame(this._raf);
        this._raf = 0;
        this.phase = "done";
      };
    }
  }

  // vals: current values inside bursts; fixedAt(i): 0..1 repaired amount
  draw(vals, fixedAt) {
    let d = "";
    for (let i = 0; i < this.n; i++)
      d += (i ? "L" : "M") + this.x(i).toFixed(1) + "," + this.y(this.inBurst[i] < 0 ? this.calm[i] : vals[i]);
    this.base.setAttribute("d", d);
    this.ranges.forEach(([i0, i1], j) => {
      let dd = "";
      for (let i = Math.max(0, i0 - 1); i <= Math.min(this.n - 1, i1 + 1); i++)
        dd += (dd ? "L" : "M") + this.x(i).toFixed(1) + "," + this.y(vals[i]);
      this.lines[j].setAttribute("d", dd);
      const p = fixedAt((i0 + i1) >> 1);
      this.lines[j].style.stroke = mix(p);
      this.bands[j].classList.toggle("fixed", p >= 1);
    });
  }

  // draws the head at headX (svg px, may run past VW to guarantee full
  // squash of the last burst) and squashes bursts behind it toward `calm`
  squashAt(headX) {
    const vals = this.before.slice();
    for (const [i0, i1] of this.ranges)
      for (let i = i0; i <= i1; i++)
        vals[i] = this.before[i] + (this.calm[i] - this.before[i]) * ease((headX - this.x(i)) / SQUASH_WIDTH);
    return vals;
  }
  renderAtHeadX(headX) {
    this._headX = headX;
    this.draw(this.squashAt(headX), (i) => ease((headX - this.x(i)) / SQUASH_WIDTH));
    this.head.setAttribute("x1", headX);
    this.head.setAttribute("x2", headX);
  }

  // latest job_progress pct (0..1, monotonic overall progress from the backend)
  setProgress(pct) {
    if (this.phase !== "progress") return; // no-op once finishing, or under reduced motion
    this.pct = Math.max(this.pct, Math.min(1, pct));
  }

  _tick(now) {
    const dt = this._last ? Math.min(0.05, (now - this._last) / 1000) : 0;
    this._last = now;
    switch (this.phase) {
      case "progress": this._stepProgress(dt); break;
      case "finish": this._stepFinish(now); break;
      case "settle": this._stepSettle(now); break;
      case "fadeAfter":
      case "fadeAbort": this._stepFade(now); break;
      default: this._raf = 0; return;
    }
    this._raf = this.phase === "done" ? 0 : requestAnimationFrame((t) => this._tick(t));
  }

  _stepProgress(dt) {
    const cap = Math.min(PROGRESS_CAP, this.pct + CREEP_EPS);
    // follow reported progress at once; only the lead beyond it is rate-limited
    this._creepTo = Math.min(cap, Math.max(this.pct, this._creepTo + CREEP_RATE * dt));
    const k = 1 - Math.exp(-dt / FOLLOW_TAU);
    this.disp = Math.min(this.disp + (this._creepTo - this.disp) * k, cap);
    this.renderAtHeadX((this.warp.indexAt(this.disp) / this.n) * VW);
  }

  _stepFinish(now) {
    const t = (now - this._phaseStart) / 1000;
    const u = Math.min(1, t / FINISH_DUR);
    this.renderAtHeadX(this._headX0 + (VW + 80 - this._headX0) * ease(u));
    if (u >= 1) this._beginSettle(now);
  }

  _beginSettle(now) {
    this._settleFrom = this.squashAt(VW + 80);
    this.phase = "settle";
    this._phaseStart = now;
  }

  _stepSettle(now) {
    const t = (now - this._phaseStart) / 1000;
    const pe = ease(Math.min(1, t / SETTLE_DUR));
    const vals = this._settleFrom.map((v, i) => v + (this._afterVals[i] - v) * pe);
    this.draw(vals, () => 1);
    if (t >= SETTLE_DUR) { this.phase = "fadeAfter"; this._phaseStart = now; }
  }

  _stepFade(now) {
    const t = (now - this._phaseStart) / 1000;
    this.head.style.opacity = Math.max(0, 1 - t / FADE_DUR);
    if (t >= FADE_DUR) this.phase = "done";
  }

  // runs the head to the end (squashing anything left) then morphs every
  // burst from whatever's currently shown to the TRUE repaired values
  _runFinish(afterValues) {
    this._afterVals = afterValues;
    this.disp0 = this.disp;
    this._headX0 = this._headX || 0;
    this.phase = "finish";
    this._phaseStart = performance.now();
    if (!this._raf) this._raf = requestAnimationFrame((t) => this._tick(t));
  }

  // repaired values arrive: finish the sweep, settle onto them, fade the head
  setAfter({ values }) {
    if (!this.before) return;
    if (this.phase === "static" || this.phase === "idle" || this.phase === "done") {
      this.draw(values, () => 1);
      this.head.style.opacity = 0;
      this.phase = "done";
      return;
    }
    this._runFinish(values);
  }

  // job_done for a status other than "done" (setAfter's own finish/settle/
  // fade already handles "done" — this is a no-op for it). "healthy": no
  // burst data ever arrives, so just let the head reach the end and fade.
  // "error"/"cancelled": stop immediately and restore the unfixed look —
  // we never actually finished repairing, so nothing should look fixed.
  stop(kind) {
    if (!this.before || this.phase === "done" || kind === "done") return;
    if (this.phase === "static" || this.phase === "idle") {
      if (kind !== "healthy") this.draw(this.before, () => 0);
      this.head.style.opacity = 0;
      this.phase = "done";
      return;
    }
    if (kind === "healthy") { this._runFinish(this.before); return; }
    if (this._raf) cancelAnimationFrame(this._raf);
    this.draw(this.before, () => 0);
    this._phaseStart = performance.now();
    this.phase = "fadeAbort";
    this._raf = requestAnimationFrame((t) => this._tick(t));
  }
}

window.o4trace = { hero, ClipTrace, buildWarp };
})();
