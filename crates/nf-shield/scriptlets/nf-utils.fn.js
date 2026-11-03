// NIGHTFALL Shield scriptlets: shared helpers (MPL-2.0, original code).
// Scriptlets run in the page's own world at document start.
function nfUtils() {
  if (window.__nfShieldUtils) return window.__nfShieldUtils;
  const fnToString = Function.prototype.toString;
  const U = {
    // uBO-compatible pattern: "/regex/flags", plain text, or "" (match all).
    re(pattern, fallbackAll) {
      if (pattern === undefined || pattern === '') return fallbackAll === false ? /^\b$/ : /^/;
      const m = /^\/(.+)\/([gimsu]*)$/.exec(pattern);
      if (m) { try { return new RegExp(m[1], m[2]); } catch (e) { return /^\b$/; } }
      return new RegExp(pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
    },
    // Value keywords accepted by set-constant and friends.
    value(raw) {
      switch (raw) {
        case 'undefined': return undefined;
        case 'false': return false;
        case 'true': return true;
        case 'null': return null;
        case "''": case '': return '';
        case '[]': case 'emptyArr': return [];
        case '{}': case 'emptyObj': return {};
        case 'noopFunc': return function () {};
        case 'trueFunc': return function () { return true; };
        case 'falseFunc': return function () { return false; };
        case 'throwFunc': return function () { throw new TypeError('nightfall'); };
        case 'emptyStr': return '';
        case 'yes': return 'yes';
        case 'no': return 'no';
        case 'noopPromiseResolve': return function () { return Promise.resolve(); };
        case 'noopPromiseReject': return function () { return Promise.reject(); };
      }
      if (/^-?\d+(\.\d+)?$/.test(raw)) {
        const n = Number(raw);
        return Math.abs(n) <= 0x7FFF ? n : undefined;
      }
      return undefined;
    },
    validValue(raw) {
      return /^(undefined|false|true|null|''|\[\]|\{\}|emptyArr|emptyObj|noopFunc|trueFunc|falseFunc|throwFunc|emptyStr|yes|no|noopPromiseResolve|noopPromiseReject|-?\d+(\.\d+)?)$/.test(raw || '');
    },
    // Make a replacement function look native.
    cloak(fn, orig) {
      try {
        Object.defineProperty(fn, 'name', { value: orig.name });
        fn.toString = function () { return fnToString.call(orig); };
      } catch (e) {}
      return fn;
    },
    // Path helpers for "a.b.[].c" style paths ("[]" and "*" match every key).
    findOwners(root, path) {
      const parts = path.split('.');
      let owners = [root];
      for (let i = 0; i < parts.length - 1; i++) {
        const next = [];
        for (const o of owners) {
          if (o === null || typeof o !== 'object') continue;
          const p = parts[i];
          if (p === '[]' || p === '*') { for (const k of Object.keys(o)) next.push(o[k]); }
          else if (Object.prototype.hasOwnProperty.call(o, p)) next.push(o[p]);
        }
        owners = next;
        if (owners.length === 0) break;
      }
      return { owners, last: parts[parts.length - 1] };
    },
    hasPath(root, path) {
      const { owners, last } = U.findOwners(root, path);
      return owners.some((o) => o !== null && typeof o === 'object' &&
        (last === '[]' || last === '*' ? Object.keys(o).length > 0 : Object.prototype.hasOwnProperty.call(o, last)));
    },
    // Remove every property matched by the space-separated prune paths, only
    // if all needle paths are present. Returns true when something changed.
    prune(obj, prunePaths, needlePaths) {
      if (obj === null || typeof obj !== 'object') return false;
      const prunes = (prunePaths || '').split(/ +/).filter(Boolean);
      const needles = (needlePaths || '').split(/ +/).filter(Boolean);
      if (prunes.length === 0) return false;
      if (needles.length && !needles.every((n) => U.hasPath(obj, n))) return false;
      let changed = false;
      for (const path of prunes) {
        const { owners, last } = U.findOwners(obj, path);
        for (const o of owners) {
          if (o === null || typeof o !== 'object') continue;
          if (last === '[]' || last === '*') {
            for (const k of Object.keys(o)) { delete o[k]; changed = true; }
            if (Array.isArray(o)) o.length = 0;
          } else if (Object.prototype.hasOwnProperty.call(o, last)) {
            delete o[last]; changed = true;
          }
        }
      }
      return changed;
    },
    // "url:/re/ method:POST" or a bare pattern (matched against the URL).
    matchProps(props, details) {
      if (!props) return true;
      for (const item of props.split(/ +/).filter(Boolean)) {
        const i = item.indexOf(':');
        let key = 'url', pat = item;
        if (i > 0 && /^[a-z]+$/i.test(item.slice(0, i))) { key = item.slice(0, i); pat = item.slice(i + 1); }
        const v = details[key];
        if (v === undefined || !U.re(pat).test(String(v))) return false;
      }
      return true;
    },
    requestDetails(input, init) {
      let url = '', method = 'GET';
      try {
        if (input instanceof Request) { url = input.url; method = input.method; }
        else url = String(input);
        if (init && init.method) method = String(init.method);
      } catch (e) {}
      return { url, method: method.toUpperCase() };
    },
    onReady(fn) {
      if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', fn, { once: true });
      else fn();
    },
    // Run fn now and whenever the DOM changes (throttled).
    watch(fn) {
      let pending = false;
      const run = () => { pending = false; try { fn(); } catch (e) {} };
      const obs = new MutationObserver(() => { if (!pending) { pending = true; setTimeout(run, 50); } });
      const start = () => { run(); obs.observe(document.documentElement, { childList: true, subtree: true, attributes: true }); };
      if (document.documentElement) start(); else document.addEventListener('readystatechange', start, { once: true });
    },
  };
  Object.defineProperty(window, '__nfShieldUtils', { value: U, enumerable: false });
  return U;
}
