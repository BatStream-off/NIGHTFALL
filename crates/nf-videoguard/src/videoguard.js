// NIGHTFALL VideoGuard content script (injected by the browser core).
// 1. Skips SponsorBlock segments of the current YouTube video (segments come
//    from the browser core, never from the page) and lets the user go back;
//    mutes "mute" segments, labels whole-video segments, offers to jump to
//    the highlight, and draws the segments on the progress bar.
// 2. Fast-forwards YouTube's own video ads that Shield did not block.
(function () {
  if (window.__nightfallVideoGuard) return;
  var state = {
    videoId: null, segments: [], skipped: {}, reverted: {}, shownPoi: false, shownFull: false,
    mute: null, timer: null, opts: { skipVideoAds: true }, ad: null, suspended: false, labels: {}
  };
  var COLORS = {
    sponsor: '#00d26a', selfpromo: '#ffd23f', interaction: '#cc66ff', intro: '#00e5ff', outro: '#0288d1',
    preview: '#6aa9ff', music_offtopic: '#ff9900', filler: '#7300ff', hook: '#3d7bff', exclusive_access: '#008a5c'
  };

  var LABELS = {
    sponsor: 'Sponsor', intro: 'Intro', outro: 'Outro', interaction: "Rappel d'abonnement",
    selfpromo: 'Autopromotion', preview: 'Aperçu', filler: 'Hors-sujet', music_offtopic: 'Partie non musicale',
    hook: 'Accroche', poi_highlight: 'Moment fort', exclusive_access: 'Accès exclusif'
  };
  function label(c) { return state.labels[c] || LABELS[c] || c; }

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
      '#nf-vg-toast button{background:#2967ff;color:#fff;border:0;border-radius:8px;padding:6px 10px;cursor:pointer;font:inherit}' +
      '#nf-vg-toast button:hover{background:#4a82ff}' +
      '#nf-vg-bar{position:absolute;left:0;right:0;top:0;bottom:0;pointer-events:none;z-index:40}' +
      '#nf-vg-bar i{position:absolute;top:0;bottom:0;opacity:.85;min-width:2px}';
    (document.head || document.documentElement).appendChild(s);
  }

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

  function clock(sec) {
    sec = Math.max(0, Math.floor(sec));
    var m = Math.floor(sec / 60), s = sec % 60;
    return m + ':' + (s < 10 ? '0' : '') + s;
  }

  function currentVideoId() {
    try {
      var u = new URL(location.href);
      if (u.pathname === '/watch') return u.searchParams.get('v');
      var m = u.pathname.match(/^\/(shorts|embed|live)\/([A-Za-z0-9_-]{11})/);
      return m ? m[2] : null;
    } catch (e) { return null; }
  }

  function ofKind(k) { return state.segments.filter(function (s) { return (s.kind || 'skip') === k; }); }

  // ---- SponsorBlock segments
  function onTime(video) {
    if (state.suspended) return;
    if (!state.videoId || currentVideoId() !== state.videoId || state.ad) return;
    var t = video.currentTime;
    var from = t, jumped = [], offered = null;
    // Skips, in start order; after a jump, the new position is checked
    // again (a segment inside another one never sends the video back).
    for (var round = 0; round < 8; round++) {
      var moved = false;
      ofKind('skip').forEach(function (s) {
        if (moved || !(t >= s.start && t < s.end - 0.3)) return;
        // After a jump, a segment already skipped is skipped again when the
        // jump lands inside it (nested segments).
        var armed = !state.skipped[s.uuid] || jumped.length > 0;
        if (s.action === 'skip' && armed && !state.reverted[s.uuid]) {
          state.skipped[s.uuid] = true;
          jumped.push(s);
          t = s.end;
          moved = true;
        } else if (s.action === 'show' && !state.skipped['show:' + s.uuid] && !offered) {
          state.skipped['show:' + s.uuid] = true;
          offered = s;
        }
      });
      if (!moved) break;
    }
    if (jumped.length) {
      video.currentTime = t;
      var names = jumped.map(function (s) { return label(s.category); }).filter(function (v, i, a) { return a.indexOf(v) === i; });
      toast('Segment ignoré : ' + names.join(', '), 'Revenir', function () {
        jumped.forEach(function (s) { state.reverted[s.uuid] = true; });
        video.currentTime = from;
      });
    } else if (offered) {
      toast(label(offered.category) + ' détecté', 'Passer', function () {
        video.currentTime = offered.end;
      });
    }
    // Mute segments: sound off inside, back as it was outside.
    var inMute = ofKind('mute').some(function (s) { return t >= s.start && t < s.end; });
    if (inMute && !state.mute) {
      state.mute = { muted: video.muted };
      video.muted = true;
      var m = ofKind('mute').filter(function (s) { return t >= s.start && t < s.end; })[0];
      toast('Son coupé : ' + label(m.category));
    } else if (!inMute && state.mute) {
      video.muted = state.mute.muted;
      state.mute = null;
    }
    // Whole-video label, once per video.
    var full = ofKind('full')[0];
    if (full && !state.shownFull) {
      state.shownFull = true;
      toast('Vidéo entière : ' + label(full.category));
    }
    // Highlight, offered once while it is still ahead.
    var poi = ofKind('poi')[0];
    if (poi && !state.shownPoi && t < poi.start - 2 && !jumped.length && !offered) {
      state.shownPoi = true;
      toast('Moment fort à ' + clock(poi.start), 'Y aller', function () { video.currentTime = poi.start; });
    }
  }

  function onSeeking(video) {
    var t = video.currentTime;
    state.segments.forEach(function (s) {
      // "Revenir" is forgotten once the user leaves the segment.
      if (t < s.start - 0.05 || t >= s.end) state.reverted[s.uuid] = false;
      // Back before a segment (or to the very start for a segment that
      // begins at 0): it can be skipped, or offered, again.
      var before = t < s.start - 0.05 || (s.start < 0.5 && t < 0.25);
      if (before && !state.reverted[s.uuid]) {
        state.skipped[s.uuid] = false;
        state.skipped['show:' + s.uuid] = false;
      }
    });
  }

  // ---- segments on the progress bar
  function drawBar(video) {
    var host = document.querySelector('.ytp-progress-bar');
    var old = document.getElementById('nf-vg-bar');
    if (old && old.parentNode !== host) old.remove();
    if (!host || !video || !isFinite(video.duration) || video.duration <= 0 || state.videoId !== currentVideoId()) return;
    css();
    var bar = document.getElementById('nf-vg-bar');
    if (!bar) {
      bar = document.createElement('div');
      bar.id = 'nf-vg-bar';
      host.appendChild(bar);
    }
    var key = state.videoId + ':' + video.duration + ':' + state.segments.length;
    if (bar.dataset.k === key) return;
    bar.dataset.k = key;
    bar.textContent = '';
    state.segments.forEach(function (s) {
      var k = s.kind || 'skip';
      if (k !== 'skip' && k !== 'mute') return;
      var i = document.createElement('i');
      i.style.left = (100 * s.start / video.duration) + '%';
      i.style.width = (100 * Math.max(0, Math.min(s.end, video.duration) - s.start) / video.duration) + '%';
      i.style.background = COLORS[s.category] || '#5ea3ff';
      i.title = label(s.category);
      bar.appendChild(i);
    });
  }

  // ---- YouTube video ads (fallback when Shield did not block them)
  function player() {
    return document.querySelector('#movie_player, .html5-video-player');
  }
  function checkAd() {
    if (state.suspended) return;
    if (!state.opts.skipVideoAds) return;
    var p = player();
    var video = p && p.querySelector('video');
    if (!p || !video) return;
    var showing = p.classList.contains('ad-showing') || p.classList.contains('ad-interrupting');
    if (showing) {
      if (!state.ad) {
        // Speed and sound chosen by the user, given back after the ad.
        state.ad = { muted: video.muted, rate: video.playbackRate || 1 };
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
      try { video.playbackRate = state.ad.rate; } catch (e) {}
      state.ad = null;
    }
  }

  function attach() {
    checkAd();
    var video = document.querySelector('video');
    if (video) drawBar(video);
    if (!video || video.__nfVg) return;
    video.__nfVg = true;
    video.addEventListener('timeupdate', function () { checkAd(); onTime(video); });
    video.addEventListener('seeking', function () { onSeeking(video); });
    video.addEventListener('durationchange', function () { drawBar(video); });
  }

  function start() {
    if (state.timer) return;
    state.timer = setInterval(attach, 500);
    attach();
  }

  // Page start: options only (video ads), segments come later.
  window.__nightfallVideoGuardInit = function (opts) {
    state.opts = opts || state.opts;
    if (opts && opts.labels) state.labels = opts.labels;
    start();
  };

  // NIGHTFALL locked: the script does nothing (no skip, no notice, no ad
  // fast-forward) until the browser core resumes it.
  window.__nightfallVideoGuardSuspend = function (on) {
    state.suspended = !!on;
  };

  // Segments for the current video (sent by the browser core).
  window.__nightfallVideoGuard = function (data) {
    if (data.labels) state.labels = data.labels;
    var same = data.videoId === state.videoId;
    state.videoId = data.videoId;
    state.segments = (data.segments || []).slice().sort(function (a, b) { return a.start - b.start; });
    if (!same) {
      // New video: everything starts again. Same video sent again (the
      // address changed): choices such as "Revenir" are kept.
      state.skipped = {};
      state.reverted = {};
      state.shownPoi = false;
      state.shownFull = false;
      var n = ofKind('skip').length + ofKind('mute').length;
      if (n > 0) toast(n === 1 ? '1 segment repéré dans cette vidéo' : n + ' segments repérés dans cette vidéo');
    }
    var bar = document.getElementById('nf-vg-bar');
    if (bar) bar.dataset.k = '';
    start();
  };
})();
