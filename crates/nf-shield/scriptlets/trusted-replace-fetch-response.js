// trusted-replace-fetch-response (trusted lists only): text replacement in
// matching fetch() response bodies.
function nfTrustedReplaceFetchResponse(pattern, replacement, propsToMatch) {
  const U = nfUtils();
  if (!pattern) return;
  const re = /^\/.+\/[gimsu]*$/.test(pattern) ? U.re(pattern) : new RegExp(pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g');
  if (pattern === '*') return;
  const origFetch = window.fetch;
  window.fetch = U.cloak(function (input, init) {
    const p = origFetch.apply(this, arguments);
    if (!U.matchProps(propsToMatch, U.requestDetails(input, init))) return p;
    return p.then((resp) => resp.clone().text().then((text) => {
      const out = text.replace(re, replacement || '');
      if (out === text) return resp;
      const r = new Response(out, { status: resp.status, statusText: resp.statusText, headers: resp.headers });
      try { Object.defineProperties(r, { url: { value: resp.url }, type: { value: resp.type } }); } catch (e) {}
      return r;
    }).catch(() => resp));
  }, origFetch);
}
