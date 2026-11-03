// NIGHTFALL Shield cosmetic runtime (MPL-2.0), injected at document start.
// Data: { css, procedural: [json...], generic, token }.
(function (data) {
  if (window.__nfShieldCosmetic) return;
  Object.defineProperty(window, '__nfShieldCosmetic', { value: true, enumerable: false });

  const styles = [];
  function addStyle(text) {
    if (!text) return;
    const s = document.createElement('style');
    s.setAttribute('data-nf', '');
    s.textContent = text;
    styles.push(s);
    const put = () => (document.head || document.documentElement).appendChild(s);
    if (document.documentElement) put(); else document.addEventListener('readystatechange', put, { once: true });
  }
  // Called by the browser core with CSS for generic class/id rules.
  Object.defineProperty(window, '__nfShieldAddCss', { value: addStyle, enumerable: false });

  addStyle(data.css);

  // ---- procedural filters (has-text, upward, matches-css, ...)
  const rules = (data.procedural || []).map((j) => { try { return JSON.parse(j); } catch (e) { return null; } }).filter(Boolean);
  function textRe(arg) {
    const m = /^\/(.+)\/([imsu]*)$/.exec(arg);
    if (m) { try { return new RegExp(m[1], m[2]); } catch (e) { return null; } }
    return { test: (s) => s.includes(arg) };
  }
  function step(nodes, op) {
    const out = [];
    const add = (n) => { if (n && !out.includes(n)) out.push(n); };
    switch (op.type) {
      case 'css-selector': {
        const sel = op.arg;
        if (nodes === null) { try { document.querySelectorAll(sel).forEach(add); } catch (e) {} return out; }
        for (const n of nodes) {
          try {
            if (/^\s*[>+~]/.test(sel)) n.querySelectorAll(':scope ' + sel).forEach(add);
            else n.querySelectorAll(sel).forEach(add);
          } catch (e) {}
        }
        return out;
      }
      case 'has-text': { const re = textRe(op.arg); if (!re) return out; for (const n of nodes) if (re.test(n.textContent || '')) add(n); return out; }
      case 'min-text-length': { const l = parseInt(op.arg, 10) || 0; for (const n of nodes) if ((n.textContent || '').length >= l) add(n); return out; }
      case 'upward': {
        const k = parseInt(op.arg, 10);
        for (const n of nodes) {
          let p = n;
          if (k > 0) { for (let i = 0; i < k && p; i++) p = p.parentElement; }
          else { p = n.parentElement && n.parentElement.closest(op.arg); }
          add(p);
        }
        return out;
      }
      case 'matches-attr': {
        const m = /^"?(.+?)"?="?(.*?)"?$/.exec(op.arg);
        if (!m) return out;
        const nameRe = textRe(m[1]), valRe = textRe(m[2]);
        for (const n of nodes) for (const a of n.attributes || []) if (nameRe.test(a.name) && valRe.test(a.value)) { add(n); break; }
        return out;
      }
      case 'matches-css': case 'matches-css-before': case 'matches-css-after': {
        const m = /^\s*([\w-]+)\s*:\s*(.+)$/.exec(op.arg);
        if (!m) return out;
        const pseudo = op.type === 'matches-css-before' ? '::before' : op.type === 'matches-css-after' ? '::after' : null;
        const re = textRe(m[2].trim());
        for (const n of nodes) if (re.test(getComputedStyle(n, pseudo).getPropertyValue(m[1]))) add(n);
        return out;
      }
      case 'matches-path': { const re = textRe(op.arg); return re && re.test(location.pathname + location.search) ? nodes.slice() : out; }
      case 'xpath': {
        for (const n of nodes) {
          try {
            const r = document.evaluate(op.arg, n, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
            for (let i = 0; i < r.snapshotLength; i++) if (r.snapshotItem(i).nodeType === 1) add(r.snapshotItem(i));
          } catch (e) {}
        }
        return out;
      }
    }
    return out;
  }
  function select(rule) {
    let nodes = null;
    for (const op of rule.selector || []) {
      nodes = step(nodes === null && op.type !== 'css-selector' ? [document.documentElement] : nodes, op);
      if (nodes.length === 0) return nodes;
    }
    return nodes || [];
  }
  function apply(rule) {
    const a = rule.action;
    for (const el of select(rule)) {
      if (!a) { el.style.setProperty('display', 'none', 'important'); continue; }
      switch (a.type) {
        case 'remove': el.remove(); break;
        case 'style': el.setAttribute('style', (el.getAttribute('style') || '') + ';' + a.arg); break;
        case 'remove-attr': el.removeAttribute(a.arg); break;
        case 'remove-class': el.classList.remove(a.arg); break;
      }
    }
  }

  // ---- generic class/id rules: report new classes and ids to the core.
  const seen = new Set();
  let pending = { c: [], i: [] };
  let sendTimer = null;
  function collect(el) {
    if (!data.generic || !el || el.nodeType !== 1) return;
    if (el.id && !seen.has('#' + el.id)) { seen.add('#' + el.id); pending.i.push(el.id); }
    for (const c of el.classList || []) if (!seen.has('.' + c)) { seen.add('.' + c); pending.c.push(c); }
  }
  function flush() {
    sendTimer = null;
    if (!pending.c.length && !pending.i.length) return;
    const msg = data.token + JSON.stringify(pending);
    pending = { c: [], i: [] };
    console.debug(msg);
  }
  function schedule() { if (!sendTimer) sendTimer = setTimeout(flush, 120); }

  let busy = false;
  function run(mutations) {
    if (busy) return;
    busy = true;
    try {
      if (data.generic) {
        if (!mutations) document.querySelectorAll('[id],[class]').forEach(collect);
        else for (const m of mutations) {
          if (m.type === 'attributes') collect(m.target);
          for (const n of m.addedNodes || []) { if (n.nodeType === 1) { collect(n); n.querySelectorAll && n.querySelectorAll('[id],[class]').forEach(collect); } }
        }
        schedule();
      }
      rules.forEach(apply);
    } catch (e) {}
    busy = false;
  }
  let queued = [];
  let raf = false;
  const obs = new MutationObserver((ms) => {
    queued.push(...ms);
    if (!raf) { raf = true; setTimeout(() => { raf = false; const q = queued; queued = []; run(q); }, 40); }
  });
  const start = () => {
    run(null);
    obs.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'id'] });
  };
  if (data.generic || rules.length) {
    if (document.documentElement) start(); else document.addEventListener('readystatechange', start, { once: true });
  }
})
