// remove-cookie (cookie-remover): delete the page's cookies whose name
// matches, now and when the page is hidden or left.
function nfRemoveCookie(needle) {
  const U = nfUtils();
  const re = U.re(needle);
  const remove = () => {
    for (const part of document.cookie.split(';')) {
      const name = part.split('=')[0].trim();
      if (!name || !re.test(name)) continue;
      const host = location.hostname;
      const domains = ['', host, '.' + host];
      const dot = host.indexOf('.');
      if (dot > 0) domains.push(host.slice(dot));
      for (const d of domains) {
        for (const path of ['/', location.pathname]) {
          document.cookie = name + '=; Max-Age=0; path=' + path + (d ? '; domain=' + d : '');
        }
      }
    }
  };
  remove();
  window.addEventListener('beforeunload', remove);
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'hidden') remove(); });
}
