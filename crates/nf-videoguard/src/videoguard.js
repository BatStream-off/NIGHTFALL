// NIGHTFALL VideoGuard content script (injected by the browser core).
// 1. Skips SponsorBlock segments of the current YouTube video (segments come
//    from the browser core, never from the page) and lets the user go back.
// 2. Fast-forwards YouTube's own video ads that Shield did not block.
(function () {
  if (window.__nightfallVideoGuard) return;
  var state = { videoId: null, segments: [], skipped: {}, timer: null, opts: { skipVideoAds: true }, ad: null };

  function css() {
    if (document.getElementById('nf-vg-style')) return;
    var s = document.createElement('style');
    s.id = 'nf-vg-style';
    s.textContent =
      '#nf-vg-toast{position:fixed;left:50%;bottom:76px;transform:translateX(-50%) translateY(8px);opacity:0;' +
      'transition:opacity .18s ease,transform .18s ease;z-index:2147483647;' +
      'background:rgba(9,12,20,.92);backdrop-filter:blur(10px);color:#e8eeff;border:1px solid rgba(94,163,255,.45);' +
      'border-radius:12px;padding:10px 12px 10px 14px;font:500 13px/1.3 system-ui,-apple-system,"Segoe UI",sans-serif;' +
      'box-shadow:0 10px 30px rgba(0,0,0,.55);display:flex;gap:12px;align-items:center}' +
      '#nf-vg-toast.on{opacity:1;transform:translateX(-50%) translateY(0)}' +
      '#nf-vg-toast .nf-vg-dot{width:8px;height:8px;border-radius:50%;background:#5ea3ff;box-shadow:0 0 10px #5ea3ff}' +
      '#nf-vg-toast button{background:#2f6dff;color:#fff;border:0;border-radius:8px;padding:6px 10px;cursor:pointer;font:inherit}' +
      '#nf-vg-toast button:hover{background:#4a82ff}';
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
    var dot = document.createElement('span');
    dot.className = 'nf-vg-dot';
    t.appendChild(dot);
    var span = document.createElement('span');
    span.textContent = 'VideoGuard · ' + text;
    t.appendChild(span);
    if (buttonText) {
      var b = document.createElement('button');
      b.textContent = buttonText;
      b.addEventListener('click', function () { onClick(); t.remove(); });
      t.appendChild(b);
    }
    document.documentElement.appendChild(t);
    requestAnimationFrame(function () { t.classList.add('on'); });
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

  // ---- SponsorBlock segments
  function onTime(video) {
    if (!state.videoId || currentVideoId() !== state.videoId || state.ad) return;
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

  // ---- YouTube video ads (fallback when Shield did not block them)
  function player() {
    return document.querySelector('#movie_player, .html5-video-player');
  }
  function checkAd() {
    if (!state.opts.skipVideoAds) return;
    var p = player();
    var video = p && p.querySelector('video');
    if (!p || !video) return;
    var showing = p.classList.contains('ad-showing') || p.classList.contains('ad-interrupting');
    if (showing) {
      if (!state.ad) {
        state.ad = { muted: video.muted };
        toast('Publicité vidéo accélérée');
      }
      video.muted = true;
      if (isFinite(video.duration) && video.duration > 0 && video.currentTime < video.duration - 0.2) {
        try { video.currentTime = video.duration - 0.1; } catch (e) {}
      }
      try { video.playbackRate = 16; } catch (e) {}
      var skip = p.querySelector('.ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-skip-ad-button, button[class*="skip-button"]');
      if (skip) skip.click();
    } else if (state.ad) {
      video.muted = state.ad.muted;
      try { video.playbackRate = 1; } catch (e) {}
      state.ad = null;
    }
  }

  function attach() {
    checkAd();
    var video = document.querySelector('video');
    if (!video || video.__nfVg) return;
    video.__nfVg = true;
    video.addEventListener('timeupdate', function () { checkAd(); onTime(video); });
    video.addEventListener('seeking', function () {
      // Seeking manually back into a segment should not re-skip at once.
      var t = video.currentTime;
      state.segments.forEach(function (s) { if (t < s.start) state.skipped[s.uuid] = false; });
    });
  }

  function start() {
    if (state.timer) return;
    state.timer = setInterval(attach, 500);
    attach();
  }

  // Page start: options only (video ads), segments come later.
  window.__nightfallVideoGuardInit = function (opts) {
    state.opts = opts || state.opts;
    start();
  };

  // Segments for the current video (sent by the browser core).
  window.__nightfallVideoGuard = function (data) {
    state.videoId = data.videoId;
    state.segments = data.segments || [];
    state.skipped = {};
    start();
    var n = state.segments.length;
    if (n > 0) toast(n === 1 ? '1 segment repéré dans cette vidéo' : n + ' segments repérés dans cette vidéo');
  };
})();
