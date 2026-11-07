// Vidéo de démonstration Widevine publique de Shaka Player (« Angel One »).
const MANIFEST = 'https://storage.googleapis.com/shaka-demo-assets/angel-one-widevine/dash.mpd';
const LICENSE = 'https://cwip-shaka-proxy.appspot.com/no_auth';
const state = window.nfPocState = { keySystem: null, currentTime: 0, decodedFrames: 0, licenses: 0, error: null };
const video = document.getElementById('v');
const label = document.getElementById('s');

function update() {
  state.currentTime = video.currentTime;
  const q = video.getVideoPlaybackQuality ? video.getVideoPlaybackQuality() : null;
  state.decodedFrames = q ? q.totalVideoFrames - q.droppedVideoFrames : 0;
  label.textContent = `${state.keySystem || 'sans DRM'} · ${state.currentTime.toFixed(1)} s · ${state.decodedFrames} images · ${state.licenses} licence(s)`;
}

(async () => {
  try {
    shaka.polyfill.installAll();
    if (!shaka.Player.isBrowserSupported()) throw new Error('Shaka : navigateur non pris en charge');
    const support = await shaka.Player.probeSupport();
    console.log('Widevine pris en charge :', JSON.stringify(support.drm['com.widevine.alpha']));
    const player = new shaka.Player();
    await player.attach(video);
    player.configure({ drm: { servers: { 'com.widevine.alpha': LICENSE } } });
    player.getNetworkingEngine().registerResponseFilter((type) => {
      if (type === shaka.net.NetworkingEngine.RequestType.LICENSE) state.licenses++;
    });
    player.addEventListener('error', (e) => { state.error = 'Shaka ' + e.detail.code; });
    await player.load(MANIFEST);
    state.keySystem = player.keySystem();
    await video.play();
  } catch (e) {
    state.error = e && e.code ? 'Shaka ' + e.code : String(e);
  }
  setInterval(update, 500);
})();
