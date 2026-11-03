// prevent-addEventListener (aeld): drop matching event listeners.
function nfPreventAddEventListener(type, needle) {
  const U = nfUtils();
  const typeRe = U.re(type);
  const re = U.re(needle);
  const proto = EventTarget.prototype;
  const orig = proto.addEventListener;
  proto.addEventListener = U.cloak(function (t, fn, ...rest) {
    let text = '';
    try { text = String(typeof fn === 'function' ? fn : fn && fn.handleEvent); } catch (e) {}
    if (typeRe.test(String(t)) && re.test(text)) return;
    return orig.call(this, t, fn, ...rest);
  }, orig);
}
