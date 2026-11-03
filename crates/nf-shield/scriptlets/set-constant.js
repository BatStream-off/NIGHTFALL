// set-constant (alias: set): define a property chain with a fixed value,
// even when the parent objects are created later by the page.
function nfSetConstant(chain, rawValue) {
  const U = nfUtils();
  if (!chain || !U.validValue(rawValue)) return;
  const value = U.value(rawValue);
  const setChain = (owner, parts) => {
    const prop = parts[0];
    if (parts.length === 1) {
      try {
        Object.defineProperty(owner, prop, {
          configurable: false, enumerable: true,
          get() { return value; },
          set() {},
        });
      } catch (e) {}
      return;
    }
    const rest = parts.slice(1);
    let current = owner[prop];
    if (current !== null && (typeof current === 'object' || typeof current === 'function')) {
      setChain(current, rest);
    }
    try {
      Object.defineProperty(owner, prop, {
        configurable: true, enumerable: true,
        get() { return current; },
        set(v) {
          current = v;
          if (v !== null && (typeof v === 'object' || typeof v === 'function')) setChain(v, rest);
        },
      });
    } catch (e) {}
  };
  setChain(window, chain.split('.'));
}
