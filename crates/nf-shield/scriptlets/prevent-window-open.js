// prevent-window-open (nowoif): block matching window.open calls.
function nfPreventWindowOpen(needle) {
  const U = nfUtils();
  let neg = false, n = needle || '';
  if (n.startsWith('!')) { neg = true; n = n.slice(1); }
  const re = U.re(n);
  const orig = window.open;
  window.open = U.cloak(function (url, ...rest) {
    if (re.test(String(url || '')) !== neg) return null;
    return orig.call(this, url, ...rest);
  }, orig);
}
