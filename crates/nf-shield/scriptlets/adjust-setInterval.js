// adjust-setInterval (nano-sib): speed up matching timers.
function nfAdjustSetInterval(needle, delay, boost) {
  const U = nfUtils();
  const re = U.re(needle);
  const d = delay === undefined || delay === '' || delay === '*' ? null : parseInt(delay, 10) || 1000;
  let b = parseFloat(boost); if (!(b >= 0.001 && b <= 50)) b = 0.05;
  const orig = window.setInterval;
  window.setInterval = U.cloak(function (fn, ms, ...rest) {
    if ((d === null || Number(ms) === d) && re.test(String(fn))) ms = Number(ms) * b;
    return orig.call(this, fn, ms, ...rest);
  }, orig);
}
