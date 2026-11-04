// json-prune-xhr-response: prune JSON bodies of matching XMLHttpRequests.
function nfJsonPruneXhrResponse(prunePaths, needlePaths, ...extra) {
  const U = nfUtils();
  if (!prunePaths) return;
  let props = '';
  for (let i = 0; i + 1 < extra.length; i += 2) if (extra[i] === 'propsToMatch') props = extra[i + 1];
  const X = XMLHttpRequest.prototype;
  const origOpen = X.open;
  X.open = U.cloak(function (method, url) {
    this.__nfMatch = U.matchProps(props, { url: String(url), method: String(method).toUpperCase() });
    return origOpen.apply(this, arguments);
  }, origOpen);
  const descText = Object.getOwnPropertyDescriptor(X, 'responseText');
  const descResp = Object.getOwnPropertyDescriptor(X, 'response');
  const pruned = (xhr, raw) => {
    if (!xhr.__nfMatch || xhr.readyState !== 4) return raw;
    if (xhr.__nfCache && xhr.__nfCache.raw === raw) return xhr.__nfCache.out;
    let out = raw;
    try {
      if (typeof raw === 'string') {
        const obj = JSON.parse(raw);
        if (U.prune(obj, prunePaths, needlePaths)) out = JSON.stringify(obj);
      } else if (raw && typeof raw === 'object') {
        U.prune(raw, prunePaths, needlePaths);
      }
    } catch (e) {}
    xhr.__nfCache = { raw, out };
    return out;
  };
  if (descText && descText.get) Object.defineProperty(X, 'responseText', { configurable: true, get() { return pruned(this, descText.get.call(this)); } });
  if (descResp && descResp.get) Object.defineProperty(X, 'response', { configurable: true, get() { return pruned(this, descResp.get.call(this)); } });
}
