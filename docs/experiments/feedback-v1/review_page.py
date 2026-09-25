"""Write a local review page embedding side-by-side comparison videos.

usage: review_page.py SPEC.json
SPEC = {"out": "target/experiments/feedback-v1/review.html", "title": "...", "intro": "...",
        "left": "label", "right": "label",
        "items": [{"clip": "0071", "name": "Throttle burst", "window": "11-21 s",
                   "look": "what to watch", "base": "0071/compare_fbn_throttle"}]}
Each item's base path is relative to the page; <base>.mp4, <base>_full.mp4 and <base>_detail.mp4
are offered as tabs when they exist. Open the written page directly from disk in a browser."""
import html
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VARIANTS = [('', 'Preview'), ('_full', 'Full resolution'), ('_detail', 'Detail crop (native pixels, top-right)')]

CSS = """
:root{--bg:#f6f5f2;--fg:#1d1d1b;--muted:#6b6a66;--card:#fff;--line:#dedbd4;--accent:#2f5d8a;--chip:#eceae4}
@media (prefers-color-scheme:dark){:root{--bg:#161615;--fg:#ecebe7;--muted:#a3a19b;--card:#20201e;--line:#34332f;--accent:#8db4dc;--chip:#2c2b28}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif}
main{max-width:1500px;margin:0 auto;padding:24px 16px 64px}h1{font-size:22px;margin:0 0 6px}
.intro{color:var(--muted);max-width:900px;margin:0 0 14px}
.legend{display:flex;gap:10px;flex-wrap:wrap;margin-bottom:10px}.legend span{background:var(--chip);border-radius:6px;padding:4px 10px}
.keys{color:var(--muted);font-size:13px;margin:0 0 20px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px 14px 10px;margin:0 0 22px}
.head{display:flex;flex-wrap:wrap;gap:8px 14px;align-items:baseline}.head h2{font-size:17px;margin:0}
.meta{color:var(--muted);font-size:13px}.look{margin:6px 0 10px}
.tabs{display:flex;gap:6px;flex-wrap:wrap;margin-bottom:8px}
button{font:inherit;font-size:13px;border:1px solid var(--line);background:var(--chip);color:var(--fg);border-radius:6px;padding:3px 10px;cursor:pointer}
button.on{background:var(--accent);border-color:var(--accent);color:var(--bg)}
.sides{display:grid;grid-template-columns:1fr 1fr;font-size:13px;color:var(--muted);text-align:center;margin-bottom:2px}
video{width:100%;height:auto;background:#000;border-radius:6px;display:block}
.ctl{display:flex;gap:14px;flex-wrap:wrap;align-items:center;margin-top:8px;font-size:13px;color:var(--muted)}
.ctl span{display:flex;gap:6px;align-items:center}
"""

JS = """
let active=null;
document.querySelectorAll('.card').forEach(card=>{
  const v=card.querySelector('video');
  card.addEventListener('mouseenter',()=>active=v);
  card.querySelectorAll('.tabs button').forEach(b=>b.onclick=()=>{
    const t=v.currentTime,p=!v.paused,r=v.playbackRate;
    card.querySelectorAll('.tabs button').forEach(x=>x.classList.toggle('on',x===b));
    v.src=b.dataset.src;v.load();
    v.addEventListener('loadedmetadata',()=>{v.currentTime=Math.min(t,v.duration-0.01);v.playbackRate=r;if(p)v.play();},{once:true});
  });
  card.querySelectorAll('.rate button').forEach(b=>b.onclick=()=>{
    v.playbackRate=+b.dataset.r;card.querySelectorAll('.rate button').forEach(x=>x.classList.toggle('on',x===b));});
  card.querySelectorAll('[data-step]').forEach(b=>b.onclick=()=>{v.pause();v.currentTime=Math.max(0,v.currentTime+(+b.dataset.step)*0.01);});
});
document.addEventListener('keydown',e=>{
  if(!active)return;
  if(e.key===' '){e.preventDefault();active.paused?active.play():active.pause();}
  if(e.key===','){active.pause();active.currentTime=Math.max(0,active.currentTime-0.01);}
  if(e.key==='.'){active.pause();active.currentTime+=0.01;}
});
"""


def card(it, spec, page_dir):
    e = html.escape
    vids = [(f"{it['base']}{suf}.mp4", label) for suf, label in VARIANTS if (page_dir / f"{it['base']}{suf}.mp4").exists()]
    if not vids:
        raise SystemExit(f"missing videos for {it['base']}")
    tabs = ''.join(f'<button data-src="{e(rel)}" class="{"on" if i == 0 else ""}">{e(label)}</button>'
                   for i, (rel, label) in enumerate(vids))
    return f'''<section class="card"><div class="head"><h2>{e(it['name'])}</h2>
<span class="meta">clip {e(it['clip'])} &middot; {e(it['window'])}</span></div>
<p class="look">{e(it['look'])}</p><div class="tabs">{tabs}</div>
<div class="sides"><div>LEFT &middot; {e(spec['left'])}</div><div>RIGHT &middot; {e(spec['right'])}</div></div>
<video src="{e(vids[0][0])}" controls loop muted playsinline preload="metadata"></video>
<div class="ctl"><span class="rate">Speed <button data-r="0.25">&frac14;&times;</button><button data-r="0.5">&frac12;&times;</button><button data-r="1" class="on">1&times;</button></span>
<span>Frame <button data-step="-1">&#9664;</button><button data-step="1">&#9654;</button></span></div></section>'''


def main(spec_path):
    spec = json.loads(Path(spec_path).read_text(encoding='utf-8'))
    out = ROOT / spec['out']
    e = html.escape
    cards = ''.join(card(it, spec, out.parent) for it in spec['items'])
    doc = f'''<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{e(spec['title'])}</title><style>{CSS}</style></head><body><main>
<h1>{e(spec['title'])}</h1><p class="intro">{e(spec['intro'])}</p>
<div class="legend"><span>LEFT: {e(spec['left'])}</span><span>RIGHT: {e(spec['right'])}</span></div>
<p class="keys">Hover a video, then: space = play/pause, <b>,</b> / <b>.</b> = step one frame (10 ms). Resolution tabs keep the current timestamp.</p>
{cards}</main><script>{JS}</script></body></html>'''
    out.write_text(doc, encoding='utf-8')
    print(out, len(spec['items']), 'items')


if __name__ == '__main__':
    main(sys.argv[1])
