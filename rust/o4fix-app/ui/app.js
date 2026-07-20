const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let settings = null;              // GuiSettings from backend
let pending = [];                 // absolute paths not yet queued
const rows = new Map();           // job id -> DOM refs
let activeJobs = 0;

const $ = (id) => document.getElementById(id);
const busy = () => activeJobs > 0;

function baseName(p) { return p.split(/[\\/]/).pop(); }

function setControls() {
  $("start").disabled = busy() || pending.length === 0;
  $("clear").disabled = busy() || rows.size === 0;
}

function addFiles(paths) {
  if (busy()) return;
  for (const p of paths || []) {
    if (pending.includes(p)) continue;
    pending.push(p);
    const li = document.createElement("li");
    li.innerHTML = `<span class="fname"></span><span class="chip">queued</span>
      <button class="cancel" hidden>cancel</button>
      <progress max="1" value="0"></progress><span class="msg"></span>`;
    li.querySelector(".fname").textContent = baseName(p);
    li.dataset.path = p;
    $("queue").appendChild(li);
  }
  setControls();
}

async function start() {
  const files = pending.slice();
  const ids = await invoke("start_queue", { files, settings });
  activeJobs = ids.length;
  const lis = [...$("queue").querySelectorAll("li")].filter(li => !li.dataset.id);
  ids.forEach((id, i) => {
    const li = lis[i];
    li.dataset.id = id;
    const cancelBtn = li.querySelector(".cancel");
    cancelBtn.hidden = false;
    cancelBtn.onclick = () => invoke("cancel_job", { id });
    rows.set(id, { li, chip: li.querySelector(".chip"),
                   bar: li.querySelector("progress"),
                   msg: li.querySelector(".msg"), cancel: cancelBtn });
  });
  pending = [];
  setControls();
}

function onProgress(e) {
  const r = rows.get(e.payload.id);
  if (!r) return;
  r.chip.textContent = e.payload.stage;
  r.chip.className = "chip " + e.payload.stage.replace(/ /g, "-");
  r.bar.value = e.payload.pct;
}

function onLog(e) {
  const r = rows.get(e.payload.id);
  const name = r ? r.li.querySelector(".fname").textContent : e.payload.id;
  $("log").textContent += `[${name}]${e.payload.line}\n`;
  $("log").scrollTop = $("log").scrollHeight;
}

function onDone(e) {
  const { id, status, message } = e.payload;
  const r = rows.get(id);
  if (!r) return;
  r.chip.textContent = status === "healthy" ? "healthy — nothing to repair" : status;
  r.chip.className = "chip " + status;
  r.cancel.hidden = true;
  if (status === "done") { r.bar.value = 1; r.msg.textContent = "→ " + message; }
  else { r.bar.hidden = true; if (message) r.msg.textContent = message; }
  activeJobs -= 1;
  setControls();
}

// ---------- settings ----------
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
function profileLabel(p) {
  return { m2: "Default (M2)", m4: "Sharp-turn (M4)", custom: "Custom" }[p];
}

function save() {
  settings.profile = detectProfile(settings.config);
  document.querySelector(`input[name=profile][value=${settings.profile}]`).checked = true;
  $("profile-badge").textContent = "Profile: " + profileLabel(settings.profile);
  invoke("save_settings", { settings });
}

function buildFields() {
  const wrap = $("fields");
  wrap.innerHTML = "";
  let fs = null, lastGroup = null;
  for (const [group, key, label, kind] of window.FIELDS) {
    if (group !== lastGroup) {
      fs = document.createElement("fieldset");
      fs.innerHTML = `<legend>${group}</legend>`;
      wrap.appendChild(fs);
      lastGroup = group;
    }
    const lab = document.createElement("label");
    lab.title = window.HELP[key];
    lab.append(label + " ");
    const val = settings.config[key];
    if (kind === "bool") {
      const cb = Object.assign(document.createElement("input"),
                               { type: "checkbox", checked: val });
      cb.onchange = () => { settings.config[key] = cb.checked; save(); };
      lab.appendChild(cb);
    } else if (kind === "pair" || kind === "pair_opt") {
      const box = document.createElement("span");
      const inputs = [0, 1].map(i => {
        const inp = Object.assign(document.createElement("input"),
          { type: "number", className: "pair", step: "any",
            value: val === null ? "" : val[i] });
        box.appendChild(inp);
        return inp;
      });
      const commit = () => {
        const a = inputs.map(x => x.value.trim());
        if (kind === "pair_opt" && a.every(x => x === "")) settings.config[key] = null;
        else settings.config[key] = [parseFloat(a[0]) || 0, parseFloat(a[1]) || 0];
        save();
      };
      inputs.forEach(x => x.onchange = commit);
      lab.appendChild(box);
    } else { // num | num_opt
      const inp = Object.assign(document.createElement("input"),
        { type: "number", step: "any", value: val === null ? "" : val });
      inp.onchange = () => {
        const v = inp.value.trim();
        settings.config[key] = (kind === "num_opt" && v === "") ? null
          : (key === "hampel_window" ? parseInt(v, 10) || 0 : parseFloat(v) || 0);
        save();
      };
      lab.appendChild(inp);
    }
    fs.appendChild(lab);
  }
}

function renderSettings() {
  buildFields();
  $("output-dir").value = settings.output_dir || "";
  $("concurrent").value = settings.concurrent_files;
  save(); // also syncs profile radio + badge
}

async function init() {
  settings = await invoke("load_settings");
  renderSettings();

  $("dropzone").onclick = async () => addFiles(await invoke("pick_files"));
  await listen("tauri://drag-drop", (e) => addFiles(e.payload.paths));
  await listen("tauri://drag-enter", () => $("dropzone").classList.add("hover"));
  await listen("tauri://drag-leave", () => $("dropzone").classList.remove("hover"));
  await listen("job_progress", onProgress);
  await listen("job_log", onLog);
  await listen("job_done", onDone);

  $("start").onclick = start;
  $("clear").onclick = () => {
    for (const [id, r] of [...rows]) {
      if (!r.cancel.hidden) continue;           // still running
      r.li.remove(); rows.delete(id);
    }
    setControls();
  };
  $("gear").onclick = () => $("settings").hidden = false;
  $("close-settings").onclick = () => $("settings").hidden = true;
  $("reset").onclick = () => {
    settings.config = JSON.parse(JSON.stringify(window.DEFAULTS));
    renderSettings();
  };
  for (const radio of document.querySelectorAll("input[name=profile]")) {
    radio.onchange = () => {
      if (radio.value === "m2")
        settings.config = JSON.parse(JSON.stringify(window.DEFAULTS));
      else if (radio.value === "m4")
        settings.config = JSON.parse(JSON.stringify(window.M4));
      renderSettings();                          // custom: keep as-is
    };
  }
  $("browse-out").onclick = async () => {
    const dir = await invoke("pick_folder");
    if (dir) { settings.output_dir = dir; $("output-dir").value = dir; save(); }
  };
  $("clear-out").onclick = () => {
    settings.output_dir = null; $("output-dir").value = ""; save();
  };
  $("output-dir").onchange = () => {
    settings.output_dir = $("output-dir").value.trim() || null; save();
  };
  $("concurrent").onchange = () => {
    settings.concurrent_files = parseInt($("concurrent").value, 10); save();
  };
}

init();
