// nobab / nofab (bab-defuser, fuckadblock-defuser): the BlockAdBlock and
// FuckAdBlock detection libraries always report « no blocker ».
function nfAdblockDetectorDefuser() {
  const make = () => {
    const Det = function () {};
    const proto = Det.prototype;
    proto.setOption = function () { return this; };
    proto.check = function () { if (typeof this._nfNot === 'function') { try { this._nfNot(); } catch (e) {} } return true; };
    proto.emitEvent = function () { return this; };
    proto.clearEvent = function () {};
    proto.on = function (detected, fn) { if (!detected && typeof fn === 'function') this._nfNot = fn; return this; };
    proto.onDetected = function () { return this; };
    proto.onNotDetected = function (fn) { if (typeof fn === 'function') { this._nfNot = fn; setTimeout(() => { try { fn(); } catch (e) {} }, 1); } return this; };
    return Det;
  };
  for (const [ctor, inst] of [['BlockAdBlock', 'blockAdBlock'], ['FuckAdBlock', 'fuckAdBlock'], ['SniffAdBlock', 'sniffAdBlock']]) {
    const Det = make();
    try {
      Object.defineProperty(window, ctor, { value: Det, configurable: false, writable: false });
      Object.defineProperty(window, inst, { value: new Det(), configurable: false, writable: false });
    } catch (e) {}
  }
}
