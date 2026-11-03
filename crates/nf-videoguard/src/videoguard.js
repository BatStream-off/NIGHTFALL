// NIGHTFALL VideoGuard content script (injected by the browser core).
// Skips SponsorBlock segments of the current YouTube video and lets the
// user go back. Segments come from the browser core, never from the page.
(function () {
  if (window.__nightfallVideoGuard) return;
  var state = { videoId: null, segments: [], skipped: {}, lastSkip: null, timer: null };

  function css() {
    if (document.getElementById('nf-vg-style')) return;
    var s = document.createElement('style');
    s.id = 'nf-vg-style';
    s.textContent =
      '#nf-vg-toast{position:fixed;left:50%;bottom:72px;transform:translateX(-50%);z-index:2147483647;' +
      'background:#0b0d12;color:#e6ecff;border:1px solid #2b6bff;border-radius:10px;padding:10px 14px;' +
      'font:14px system-ui,sans-serif;box-shadow:0 6px 24px rgba(0,0,0,.5);display:flex;gap:12px;align-items:center}' +
      '#nf-vg-toast button{background:#2b6bff;color:#fff;border:0;border-radius:6px;padding:6px 10px;cursor:pointer;font:inherit}';
    (document.head || document.documentElement).appendChild(s);
  }

  var LABELS = {
    sponsor: 'Sponsor', intro: 'Intro', outro: 'Outro', interaction: "Rappel d'abonnement",
    selfpromo: 'Autopromotion', preview: 'Aperçu', filler: 'Hors-sujet', music_offtopic: 'Partie non musicale'
  };

  function toast(text, buttonText, onClick) {
    css();
    var old = document.getElementById('nf-vg-toast');
    if (old) old.remove();
    var t = document.createElement('div');
    t.id = 'nf-vg-toast';
    var span = document.createElement('span');
    span.textContent = text;
    t.appendChild(span);
    if (buttonText) {
      var b = document.createElement('button');
      b.textContent = buttonText;
      b.addEventListener('click', function () { onClick(); t.remove(); });
      t.appendChild(b);
    }
    document.documentElement.appendChild(t);
    setTimeout(function () { if (t.parentNode) t.remove(); }, 6000);
  }

  function currentVideoId() {
    try {
      var u = new URL(location.href);
      if (u.pathname === '/watch') return u.searchParams.get('v');
      var m = u.pathname.match(/^\/(shorts|embed|live)\/([A-Za-z0-9_-]{11})/);
      return m ? m[2] : null;
    } catch (e) { return null; }
  }

  function onTime(video) {
    if (currentVideoId() !== state.videoId) return;
    var t = video.currentTime;
    for (var i = 0; i < state.segments.length; i++) {
      var s = state.segments[i];
      if (t >= s.start && t < s.end - 0.3) {
        if (s.action === 'skip' && !state.skipped[s.uuid]) {
          state.skipped[s.uuid] = true;
          var from = t;
          video.currentTime = s.end;
          toast('Segment ignoré : ' + (LABELS[s.category] || s.category), 'Revenir', function () {
            video.currentTime = from;
          });
        } else if (s.action === 'show' && !state.skipped['show:' + s.uuid]) {
          state.skipped['show:' + s.uuid] = true;
          toast((LABELS[s.category] || s.category) + ' détecté', 'Passer', function () {
            video.currentTime = s.end;
          });
        }
      }
    }
  }

  function attach() {
    var video = document.querySelector('video');
    if (!video || video.__nfVg) return;
    video.__nfVg = true;
    video.addEventListener('timeupdate', function () { onTime(video); });
    video.addEventListener('seeking', function () {
      // Seeking manually back into a segment should not re-skip at once.
      var t = video.currentTime;
      state.segments.forEach(function (s) { if (t < s.start) state.skipped[s.uuid] = false; });
    });
  }

  window.__nightfallVideoGuard = function (data) {
    state.videoId = data.videoId;
    state.segments = data.segments || [];
    state.skipped = {};
    attach();
    if (!state.timer) state.timer = setInterval(attach, 1500);
  };
})();
