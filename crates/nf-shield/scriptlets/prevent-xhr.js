// prevent-xhr (no-xhr-if): matching XMLHttpRequests complete empty, unsent.
function nfPreventXhr(props) {
  const U = nfUtils();
  if (props === undefined) return;
  const X = XMLHttpRequest.prototype;
  const origOpen = X.open, origSend = X.send;
  X.open = U.cloak(function (method, url) {
    this.__nfBlock = U.matchProps(props, { url: String(url), method: String(method).toUpperCase() });
    return origOpen.apply(this, arguments);
  }, origOpen);
  X.send = U.cloak(function () {
    if (!this.__nfBlock) return origSend.apply(this, arguments);
    const xhr = this;
    try {
      Object.defineProperties(xhr, {
        readyState: { value: 4, configurable: true }, status: { value: 200, configurable: true },
        responseText: { value: '', configurable: true }, response: { value: '', configurable: true },
      });
    } catch (e) {}
    setTimeout(() => {
      for (const t of ['readystatechange', 'load', 'loadend']) { try { xhr.dispatchEvent(new Event(t)); } catch (e) {} }
    }, 1);
  }, origSend);
}
