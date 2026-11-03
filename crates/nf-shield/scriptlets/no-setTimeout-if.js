// no-setTimeout-if (nostif): neutralise matching setTimeout calls.
function nfNoSetTimeoutIf(needle, delay) {
  const U = nfUtils();
  let neg = false, n = needle || '';
  if (n.startsWith('!')) { neg = true; n = n.slice(1); }
  const re = U.re(n);
  let dNeg = false, d = delay || '';
  if (d.startsWith('!')) { dNeg = true; d = d.slice(1); }
  const dv = d === '' ? null : parseInt(d, 10);
  const orig = window.setTimeout;
  window.setTimeout = U.cloak(function (fn, ms, ...rest) {
    const text = String(fn);
    let block = re.test(text) !== neg;
    if (dv !== null) block = block && ((Number(ms) === dv) !== dNeg);
    if (block) return orig.call(this, function () {}, ms);
    return orig.call(this, fn, ms, ...rest);
  }, orig);
}
