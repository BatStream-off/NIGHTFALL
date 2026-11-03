// abort-on-property-read (aopr): throw when the page reads a property.
function nfAbortOnPropertyRead(chain) {
  if (!chain) return;
  const err = () => { throw new ReferenceError('nightfall'); };
  const parts = chain.split('.');
  const trap = (owner, i) => {
    const prop = parts[i];
    if (i === parts.length - 1) {
      try { Object.defineProperty(owner, prop, { configurable: false, get: err, set() {} }); } catch (e) {}
      return;
    }
    let v = owner[prop];
    if (v && typeof v === 'object') { trap(v, i + 1); return; }
    try {
      Object.defineProperty(owner, prop, { configurable: true, get() { return v; }, set(n) { v = n; if (n && typeof n === 'object') trap(n, i + 1); } });
    } catch (e) {}
  };
  trap(window, 0);
}
