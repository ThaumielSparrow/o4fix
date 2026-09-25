const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let settings = null;              // GuiSettings from backend
let pending = [];                 // clips added but not started: { path, clip }
const rows = new Map();           // job id -> clip
let activeJobs = 0;
let starting = false;
// events that can arrive before start()'s row registration runs (instant-error
// jobs emit job_done synchronously from the backend thread) — buffered and
// replayed once the row exists.
let earlyEvents = [];

const $ = (id) => document.getElementById(id);
const busy = () => starting || activeJobs > 0;
const baseName = (p) => p.split(/[\\/]/).pop();
const plural = (n, w) => `${n} ${w}${n === 1 ? "" : "s"}`;

// backend stage/status -> what the user reads
const STATUS = {
  queued: "Ready",
  waiting: "Waiting",
  analyzing: "Reading telemetry",
  "measuring motion": "Measuring video motion",
  patching: "Patching bursts",
  refining: "Refining",
  verifying: "Writing file",
  done: "Repaired",
  healthy: "No repair needed",
  error: "Failed",
  cancelled: "Cancelled",
};

// ---------- clip rows ----------
function makeClip(path) {
  const li = document.createElement("li");
  li.className = "clip queued";
  li.innerHTML = `
    <div class="clip-head">
      <span class="fname"></span>
      <span class="status"><span class="dot"></span><span class="stxt"></span><span class="pct"></span></span>
      <button class="ghost act"></button>
    </div>
    <div class="trace"><div class="progress"></div></div>
    <div class="clip-foot">
      <span class="facts"></span><span class="msg"></span>
      <button class="toggle-log" hidden>Details</button>
    </div>
    <pre class="log" hidden></pre>`;
  const q = (s) => li.querySelector(s);
  const clip = {
    li, path, id: null, cancelling: false, finished: false,
    name: q(".fname"), stxt: q(".stxt"), pct: q(".pct"), act: q(".act"),
    trace: new window.o4trace.ClipTrace(q(".trace")), bar: q(".progress"),
    facts: q(".facts"), msg: q(".msg"), logBtn: q(".toggle-log"), log: q(".log"),
  };
  clip.name.textContent = baseName(path);
  clip.name.title = path;
  clip.logBtn.onclick = () => {
    clip.log.hidden = !clip.log.hidden;
    clip.logBtn.textContent = clip.log.hidden ? "Details" : "Hide details";
  };
  setState(clip, "queued");
  clip.act.textContent = "Remove";
  clip.act.onclick = () => {
    pending = pending.filter(p => p.clip !== clip);
    li.remove();
    refresh();
  };
  return clip;
}

function setState(clip, state) {
  clip.li.className = "clip " + stateClass(state);
  clip.stxt.textContent = STATUS[state] ?? state;
}
function stateClass(state) {
  if (["done", "healthy", "error", "cancelled", "queued", "waiting"].includes(state))
    return state === "queued" || state === "waiting" ? "waiting" : state;
  return "working";
}

function renderFacts(clip, { t0, t1, bursts }) {
  const f = [`<span><b>${(t1 - t0).toFixed(1)}</b> s</span>`];
  if (bursts.length) {
    const secs = bursts.reduce((a, [s, e]) => a + (e - s), 0);
    f.push(`<span><b>${bursts.length}</b> noise ${bursts.length === 1 ? "burst" : "bursts"}</span>`);
    f.push(`<span><b>${secs.toFixed(1)}</b> s affected</span>`);
  }
  clip.facts.innerHTML = f.join("");
}

// ---------- queue ----------
function refresh() {
  const clips = $("queue").children.length;
  $("empty").hidden = clips > 0;
  $("queue").hidden = clips === 0;
  $("add").hidden = clips === 0 || busy();
  const finished = [...rows.values()].filter(c => c.finished).length;
  $("clear").disabled = busy() || finished === 0;
  const start = $("start");
  start.disabled = busy() || pending.length === 0;
  if (busy()) {
    start.textContent = `Working… ${finished} of ${rows.size} finished`;
  } else {
    start.textContent = pending.length === 1 ? "Repair clip"
      : pending.length ? `Repair ${plural(pending.length, "clip")}` : "Repair clips";
  }
}

function addFiles(paths) {
  if (busy()) return;
  for (const p of paths || []) {
    if (pending.some(x => x.path === p)) continue;
    const clip = makeClip(p);
    pending.push({ path: p, clip });
    $("queue").appendChild(clip.li);
  }
  refresh();
}

async function start() {
  if (busy() || pending.length === 0) return;
  starting = true;
  refresh();
  const batch = pending.slice();
  try {
    const ids = await invoke("start_queue", { files: batch.map(p => p.path), settings });
    activeJobs = ids.length;
    ids.forEach((id, i) => {
      const clip = batch[i].clip;
      clip.id = id;
      setState(clip, "waiting");
      clip.act.textContent = "Cancel";
      clip.act.onclick = () => {
        clip.cancelling = true;
        clip.stxt.textContent = "Cancelling…";
        clip.act.disabled = true;
        invoke("cancel_job", { id });
      };
      rows.set(id, clip);
    });
    pending = [];
    const replay = earlyEvents;
    earlyEvents = [];
    for (const [fn, e] of replay) fn(e);
  } catch (error) {
    for (const { clip } of batch) {
      clip.msg.textContent = `Could not start repair: ${error}`;
      clip.li.classList.add("error");
    }
  } finally {
    starting = false;
    refresh();
  }
}

function onProgress(e) {
  const clip = rows.get(e.payload.id);
  if (!clip) { earlyEvents.push([onProgress, e]); return; }
  if (clip.finished) return;
  if (!clip.cancelling) setState(clip, e.payload.stage);
  const pct = Math.round(e.payload.pct * 100);
  clip.pct.textContent = `${pct}%`;
  clip.bar.style.width = `${pct}%`;
}

function onLog(e) {
  const clip = rows.get(e.payload.id);
  if (!clip) { earlyEvents.push([onLog, e]); return; }
  const line = e.payload.line;
  clip.log.textContent += line.replace(/^\s+/, "") + "\n";
  clip.log.scrollTop = clip.log.scrollHeight;
  clip.logBtn.hidden = false;
}

function onTrace(e) {
  const clip = rows.get(e.payload.id);
  if (!clip) { earlyEvents.push([onTrace, e]); return; }
  if (e.payload.kind === "before") {
    clip.trace.setBefore(e.payload);
    renderFacts(clip, e.payload);
  } else {
    clip.trace.setAfter(e.payload);
  }
}

function onDone(e) {
  const { id, status, message } = e.payload;
  const clip = rows.get(id);
  if (!clip) { earlyEvents.push([onDone, e]); return; }
  clip.finished = true;
  setState(clip, status);
  clip.pct.textContent = "";
  clip.act.disabled = false;
  if (status === "done") {
    clip.msg.textContent = `Saved as ${baseName(message)}`;
    clip.msg.title = message;
    clip.act.textContent = "Show in folder";
    clip.act.onclick = () => invoke("reveal_file", { path: message })
      .catch((err) => { clip.msg.textContent = String(err); });
  } else {
    clip.act.hidden = true;
    if (status === "healthy") {
      clip.msg.textContent = "No noise bursts found. The original file is fine to use as is.";
    } else if (message) {
      clip.msg.textContent = message;
    }
    if (status === "error" && !clip.log.textContent) clip.logBtn.hidden = true;
  }
  activeJobs -= 1;
  refresh();
}

// ---------- settings ----------
const clone = (o) => JSON.parse(JSON.stringify(o));
function cfgEquals(a, b) {
  // key-by-key: independent of JSON key order in settings.json
  return Object.keys(window.DEFAULTS)
    .every(k => JSON.stringify(a[k]) === JSON.stringify(b[k]));
}
function detectProfile(cfg) {
  if (cfgEquals(cfg, window.DEFAULTS)) return "m2";
  if (cfgEquals(cfg, window.M4)) return "m4";
  return "custom";
}
const PROFILE_NAME = { m2: "Standard", m4: "Sharp turns", custom: "Custom" };

function save() {
  // Custom is sticky once chosen, even while its values still equal a preset;
  // otherwise the profile follows the values (editing a preset makes it Custom)
  if (settings.profile !== "custom") settings.profile = detectProfile(settings.config);
  document.querySelector(`input[name=profile][value=${settings.profile}]`).checked = true;
  // tuning dots mark values that differ from the chosen base profile
  const base = settings.profile === "m4" ? window.M4 : window.DEFAULTS;
  for (const el of document.querySelectorAll(".field[data-key]")) {
    const k = el.dataset.key;
    el.classList.toggle("changed",
      JSON.stringify(settings.config[k]) !== JSON.stringify(base[k]));
  }
  renderSummary();
  invoke("save_settings", { settings });
}

function renderSummary() {
  const where = settings.output_dir ? `saving to ${baseName(settings.output_dir)}`
                                    : "saving next to originals";
  $("summary").innerHTML =
    `<b>${PROFILE_NAME[settings.profile]}</b> profile, refinement ` +
    `<b>${settings.config.refine ? "on" : "off"}</b>, ${where}`;
}

function numInput(val, onCommit) {
  const inp = Object.assign(document.createElement("input"),
    { type: "number", step: "any", value: val === null || val === undefined ? "" : val });
  inp.onchange = onCommit;
  return inp;
}

function buildField(key, label, kind, unit) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  wrap.dataset.key = key;
  const row = document.createElement("label");
  row.className = "field-row";
  const name = document.createElement("span");
  name.textContent = label;
  row.appendChild(name);
  const val = settings.config[key];
  if (kind === "bool") {
    const cb = Object.assign(document.createElement("input"),
                             { type: "checkbox", checked: val });
    cb.onchange = () => { settings.config[key] = cb.checked; save(); };
    row.appendChild(cb);
  } else {
    const box = document.createElement("span");
    box.className = "num";
    if (kind === "pair" || kind === "pair_opt") {
      const inputs = [];
      const commit = () => {
        const a = inputs.map(x => x.value.trim());
        if (kind === "pair_opt" && a.every(x => x === "")) settings.config[key] = null;
        else settings.config[key] = [parseFloat(a[0]) || 0, parseFloat(a[1]) || 0];
        save();
      };
      [0, 1].forEach(i => {
        const inp = numInput(val === null ? null : val[i], commit);
        inp.style.width = "56px";
        if (kind === "pair_opt") inp.placeholder = "auto";
        inputs.push(inp);
        if (i === 1) box.append(Object.assign(document.createElement("span"),
                                              { className: "sep", textContent: "–" }));
        box.appendChild(inp);
      });
    } else { // num | num_opt
      const inp = numInput(val, () => {
        const v = inp.value.trim();
        settings.config[key] = (kind === "num_opt" && v === "") ? null
          : (key === "hampel_window" ? parseInt(v, 10) || 0 : parseFloat(v) || 0);
        save();
      });
      if (kind === "num_opt") inp.placeholder = "auto";
      box.appendChild(inp);
    }
    box.append(Object.assign(document.createElement("span"),
                             { className: "unit", textContent: unit }));
    row.appendChild(box);
  }
  wrap.appendChild(row);
  const help = document.createElement("p");
  help.className = "help";
  // HELP doubles as CLI-style text ("s, padding around…"); the unit is
  // already beside the input, so drop it and sentence-case the rest
  const h = window.HELP[key].replace(/^(s|ms|Hz|deg\/s(\^2)?),\s*/, "");
  help.textContent = h.charAt(0).toUpperCase() + h.slice(1) + ".";
  wrap.appendChild(help);
  return wrap;
}

function buildFields() {
  const host = $("fields");
  host.innerHTML = "";
  let group = null, lastGroup = null;
  for (const [g, key, label, kind, unit] of window.FIELDS) {
    if (g !== lastGroup) {
      group = document.createElement("div");
      group.className = "group";
      group.innerHTML = `<h4></h4>`;
      group.firstChild.textContent = g;
      host.appendChild(group);
      lastGroup = g;
    }
    group.appendChild(buildField(key, label, kind, unit));
  }
  // refinement gets a prominent switch of its own
  const slot = $("refine-slot");
  slot.innerHTML = `
    <label class="switch-row field" data-key="refine">
      <span class="txt">Refine leftover judder
        <span>Re-measures each repaired burst from the video and corrects what's left.
        Recommended. Adds time in proportion to burst length.</span></span>
      <input type="checkbox" class="switch" role="switch">
    </label>`;
  const sw = slot.querySelector("input");
  sw.checked = settings.config.refine;
  sw.onchange = () => { settings.config.refine = sw.checked; save(); };
}

function renderSettings() {
  buildFields();
  $("output-dir").textContent = settings.output_dir || "Next to each original";
  $("output-dir").title = settings.output_dir || "";
  $("output-dir").classList.toggle("set", !!settings.output_dir);
  $("clear-out").hidden = !settings.output_dir;
  for (const b of $("concurrent").children)
    b.setAttribute("aria-checked", String(+b.dataset.v === settings.concurrent_files));
  save(); // also syncs profile radio + summary
}

function openSettings(open) {
  $("settings").hidden = !open;
  $("scrim").hidden = !open;
  $("gear").setAttribute("aria-expanded", String(open));
  if (open) $("close-settings").focus();
}

async function init() {
  window.o4trace.hero($("hero-trace"));
  settings = await invoke("load_settings");
  renderSettings();
  refresh();

  const pick = async () => addFiles(await invoke("pick_files"));
  $("dropzone").onclick = pick;
  $("add").onclick = pick;
  await listen("tauri://drag-drop", (e) => {
    $("drop-veil").hidden = true;
    addFiles(e.payload.paths);
  });
  await listen("tauri://drag-enter", () => {
    if (busy()) return;
    $("drop-veil").hidden = false;
  });
  await listen("tauri://drag-leave", () => {
    $("drop-veil").hidden = true;
  });
  await listen("job_progress", onProgress);
  await listen("job_log", onLog);
  await listen("job_trace", onTrace);
  await listen("job_done", onDone);

  $("start").onclick = start;
  $("clear").onclick = () => {
    for (const [id, clip] of [...rows]) {
      if (!clip.finished) continue;
      clip.li.remove(); rows.delete(id);
    }
    refresh();
  };
  $("gear").onclick = () => openSettings(true);
  $("summary").onclick = () => openSettings(true);
  $("close-settings").onclick = () => openSettings(false);
  $("scrim").onclick = () => openSettings(false);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !$("settings").hidden) openSettings(false);
  });
  $("reset").onclick = () => {
    settings.profile = "m2";
    settings.config = clone(window.DEFAULTS);
    renderSettings();
  };
  for (const radio of document.querySelectorAll("input[name=profile]")) {
    radio.onchange = () => {
      settings.profile = radio.value;
      if (radio.value === "m2") settings.config = clone(window.DEFAULTS);
      else if (radio.value === "m4") settings.config = clone(window.M4);
      else document.querySelector(".tuning").open = true;
      renderSettings();                          // custom: keep values as-is
    };
  }
  $("browse-out").onclick = async () => {
    const dir = await invoke("pick_folder");
    if (dir) { settings.output_dir = dir; renderSettings(); }
  };
  $("clear-out").onclick = () => { settings.output_dir = null; renderSettings(); };
  for (const b of $("concurrent").children) {
    b.setAttribute("role", "radio");
    b.onclick = () => { settings.concurrent_files = +b.dataset.v; renderSettings(); };
  }
}

init();
