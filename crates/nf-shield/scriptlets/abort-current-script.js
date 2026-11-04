// abort-current-script (acs): throw when an inline/current script whose text
// matches the needle accesses the property chain.
function nfAbortCurrentScript(chain, needle) {
  const U = nfUtils();
  if (!chain) return;
  const re = U.re(needle);
  const parts = chain.split('.');
  let owner = window;
  for (let i = 0; i < parts.length - 1; i++) {
    owner = owner[parts[i]];
    if (!owner || (typeof owner !== 'object' && typeof owner !== 'function')) return;
  }
  const prop = parts[parts.length - 1];
  const desc = Object.getOwnPropertyDescriptor(owner, prop);
  let value = desc && !desc.get ? desc.value : undefined;
  const check = () => {
    const s = document.currentScript;
    if (s instanceof HTMLScriptElement && re.test(s.src || s.textContent || '')) throw new ReferenceError('nightfall');
  };
  try {
    Object.defineProperty(owner, prop, {
      configurable: true,
      get() { check(); return desc && desc.get ? desc.get.call(this) : value; },
      set(v) { check(); if (desc && desc.set) desc.set.call(this, v); else value = v; },
    });
  } catch (e) {}
}
