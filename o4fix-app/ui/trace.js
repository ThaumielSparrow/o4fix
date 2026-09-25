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

class ClipTrace {
  constructor(host) {
    this.host = host;
    this.svg = el("svg", { viewBox: `0 0 ${VW} ${VH}`, preserveAspectRatio: "none", "aria-hidden": "true" }, host);
    this.before = null;
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

  // repaired values arrive: sweep across the clip, flattening each burst
  setAfter({ values }) {
    if (!this.before) return;
    const from = this.before, to = values;
    const T = 1.6, dur = T + 0.5;
    const hx = (t) => Math.min(1, t / T) * (VW + 80);
    const p = (t, i) => ease((hx(t) - this.x(i)) / 80);
    this.cancel();
    this.cancel = animate((t) => {
      const cur = from.map((v, i) => v + (to[i] - v) * p(t, i));
      this.draw(cur, (i) => p(t, i));
      this.head.setAttribute("x1", hx(t));
      this.head.setAttribute("x2", hx(t));
      this.head.style.opacity = t < T ? 1 : Math.max(0, 1 - (t - T) / 0.3);
    }, dur);
  }
}

window.o4trace = { hero, ClipTrace };
})();
