// Shared helpers for NIGHTFALL internal pages.
'use strict';

async function api(cmd, body) {
  try {
    let r;
    // A refused call (403) was not executed, so it is safe to retry: this
    // covers the first call of a page made before its address is recorded.
    for (let attempt = 0; attempt < 4; attempt++) {
      r = await fetch('nightfall://app/api/' + cmd, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body || {}),
      });
      if (r.status !== 403) break;
      await new Promise((ok) => setTimeout(ok, 200));
    }
    if (r.status === 403) return { ok: false, error: 'interdit' };
    return await r.json();
  } catch (e) {
    console.error('NIGHTFALL api ' + cmd + ' : ' + e);
    return { ok: false, error: String(e) };
  }
}

function el(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (k === 'class') e.className = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'text') e.textContent = v;
    else if (v === true) e.setAttribute(k, '');
    else if (v !== false && v != null) e.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c == null) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

const $ = (sel, root) => (root || document).querySelector(sel);

function toast(msg, isError) {
  const t = el('div', { class: 'toast' + (isError ? ' error' : ''), text: msg });
  document.body.append(t);
  setTimeout(() => t.remove(), 3500);
}

function fmtDate(unix) {
  if (!unix) return '';
  return new Date(unix * 1000).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' });
}

function fmtBytes(n) {
  if (n == null || n < 0) return '?';
  const u = ['o', 'Ko', 'Mo', 'Go', 'To'];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return (i ? n.toFixed(1) : n) + ' ' + u[i];
}

function host(url) {
  try { return new URL(url).host; } catch (e) { return url; }
}

function check(res, okMsg) {
  if (res && res.ok) { if (okMsg) toast(okMsg); return true; }
  toast((res && res.error) || 'Erreur', true);
  return false;
}

function openPage(page) { return api('page.open', { page }); }

// Report activity (auto-lock) from internal pages too.
let __nfLastPing = 0;
['keydown', 'mousedown'].forEach((ev) => document.addEventListener(ev, () => {
  const now = Date.now();
  if (now - __nfLastPing > 20000) { __nfLastPing = now; api('ui.activity'); }
}, true));

// Chromium zooms per host and every NIGHTFALL page shares one (the toolbar
// included): Ctrl+wheel zoom is disabled on internal pages.
document.addEventListener('wheel', (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
