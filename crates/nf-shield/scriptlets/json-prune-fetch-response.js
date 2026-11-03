// json-prune-fetch-response: prune JSON bodies of matching fetch() responses.
function nfJsonPruneFetchResponse(prunePaths, needlePaths, ...extra) {
  const U = nfUtils();
  if (!prunePaths) return;
  let props = '';
  for (let i = 0; i + 1 < extra.length; i += 2) if (extra[i] === 'propsToMatch') props = extra[i + 1];
  const origFetch = window.fetch;
  window.fetch = U.cloak(function (input, init) {
    const p = origFetch.apply(this, arguments);
    if (!U.matchProps(props, U.requestDetails(input, init))) return p;
    return p.then((resp) => resp.clone().json().then((obj) => {
      if (!U.prune(obj, prunePaths, needlePaths)) return resp;
      const out = new Response(JSON.stringify(obj), { status: resp.status, statusText: resp.statusText, headers: resp.headers });
      try { Object.defineProperties(out, { url: { value: resp.url }, type: { value: resp.type }, redirected: { value: resp.redirected } }); } catch (e) {}
      return out;
    }).catch(() => resp));
  }, origFetch);
}
