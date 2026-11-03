// prevent-fetch (no-fetch-if): matching fetch() calls get an empty response.
function nfPreventFetch(props, body) {
  const U = nfUtils();
  if (props === undefined) return;
  const origFetch = window.fetch;
  window.fetch = U.cloak(function (input, init) {
    if (!U.matchProps(props, U.requestDetails(input, init))) return origFetch.apply(this, arguments);
    const text = body === 'emptyArr' ? '[]' : body === 'emptyObj' ? '{}' : '';
    return Promise.resolve(new Response(text, { status: 200, statusText: 'OK' }));
  }, origFetch);
}
