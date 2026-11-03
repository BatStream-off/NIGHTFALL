// trusted-replace-xhr-response (trusted lists only): text replacement in
// matching XMLHttpRequest responses.
function nfTrustedReplaceXhrResponse(pattern, replacement, propsToMatch) {
  const U = nfUtils();
  if (!pattern || pattern === '*') return;
  const re = /^\/.+\/[gimsu]*$/.test(pattern) ? U.re(pattern) : new RegExp(pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g');
  const X = XMLHttpRequest.prototype;
  const origOpen = X.open;
  X.open = U.cloak(function (method, url) {
    this.__nfRepl = U.matchProps(propsToMatch, { url: String(url), method: String(method).toUpperCase() });
    return origOpen.apply(this, arguments);
  }, origOpen);
  for (const prop of ['responseText', 'response']) {
    const d = Object.getOwnPropertyDescriptor(X, prop);
    if (!d || !d.get) continue;
    Object.defineProperty(X, prop, {
      configurable: true,
      get() {
        const raw = d.get.call(this);
        if (!this.__nfRepl || this.readyState !== 4 || typeof raw !== 'string') return raw;
        return raw.replace(re, replacement || '');
      },
    });
  }
}
