// abort-on-stack-trace (aost): throw when a property is read or written
// from code whose stack trace matches the pattern (a given script file).
function nfAbortOnStackTrace(chain, needle) {
  const U = nfUtils();
  if (!chain) return;
  const re = U.re(needle);
  const check = () => {
    let stack = '';
    try { stack = new Error().stack || ''; } catch (e) {}
    const lines = stack.split('\n').slice(2).join('\n');
    if (re.test(lines)) throw new ReferenceError('nightfall');
  };
  const parts = chain.split('.');
  const trap = (owner, i) => {
    const prop = parts[i];
    let v = owner[prop];
    if (i === parts.length - 1) {
      try {
        Object.defineProperty(owner, prop, { configurable: true, get() { check(); return v; }, set(n) { check(); v = n; } });
      } catch (e) {}
      return;
    }
    if (v && typeof v === 'object') { trap(v, i + 1); return; }
    try {
      Object.defineProperty(owner, prop, { configurable: true, get() { return v; }, set(n) { v = n; if (n && typeof n === 'object') trap(n, i + 1); } });
    } catch (e) {}
  };
  trap(window, 0);
}
