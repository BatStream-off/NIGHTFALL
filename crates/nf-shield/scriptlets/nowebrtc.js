// nowebrtc: the page cannot open WebRTC connections (often used to read
// local network addresses for fingerprinting).
function nfNoWebRtc() {
  const U = nfUtils();
  const names = ['RTCPeerConnection', 'webkitRTCPeerConnection', 'mozRTCPeerConnection'];
  for (const n of names) {
    const Orig = window[n];
    if (typeof Orig !== 'function') continue;
    const Fake = function () {
      return { close() {}, createDataChannel() { return { close() {} }; }, createOffer() { return Promise.reject(new DOMException('nightfall', 'NotSupportedError')); },
        setLocalDescription() { return Promise.resolve(); }, setRemoteDescription() { return Promise.resolve(); }, addEventListener() {}, removeEventListener() {}, getStats() { return Promise.resolve(new Map()); } };
    };
    Fake.prototype = Orig.prototype;
    try { Object.defineProperty(window, n, { value: U.cloak(Fake, Orig), configurable: true, writable: true }); } catch (e) {}
  }
}
