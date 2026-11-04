// adjust-setTimeout (nano-stb): speed up matching timers.
function nfAdjustSetTimeout(needle, delay, boost) {
  const U = nfUtils();
  const re = U.re(needle);
  const d = delay === undefined || delay === '' || delay === '*' ? null : parseInt(delay, 10) || 1000;
  let b = parseFloat(boost); if (!(b >= 0.001 && b <= 50)) b = 0.05;
  const orig = window.setTimeout;
  window.setTimeout = U.cloak(function (fn, ms, ...rest) {
    if ((d === null || Number(ms) === d) && re.test(String(fn))) ms = Number(ms) * b;
    return orig.call(this, fn, ms, ...rest);
  }, orig);
}
