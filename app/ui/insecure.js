'use strict';
// HTTPS-First warning: the address arrives in the fragment (http:// only).
const target = (() => {
  try { const u = decodeURIComponent(location.hash.slice(1)); return u.startsWith('http://') ? u : ''; } catch (e) { return ''; }
})();
$('#host').textContent = target ? host(target) : 'Ce site';
$('#go').disabled = !target;
$('#go').addEventListener('click', () => { if (target) api('https.proceed', { url: target }); });
$('#back').addEventListener('click', () => api('nav.home'));
$('#back').focus();
