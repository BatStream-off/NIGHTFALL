// href-sanitizer: replace the address of matching links by the real
// destination they hide (their text, a query parameter, or an attribute),
// so that a click skips the tracking redirect.
function nfHrefSanitizer(selector, source) {
  const U = nfUtils();
  if (!selector) return;
  const src = source || 'text';
  const valid = (u) => { try { const x = new URL(u, location.href); return /^https?:$/.test(x.protocol) ? x.href : ''; } catch (e) { return ''; } };
  const target = (a) => {
    if (src === 'text') return valid((a.textContent || '').trim());
    if (src.startsWith('?')) { try { return valid(new URL(a.href).searchParams.get(src.slice(1)) || ''); } catch (e) { return ''; } }
    if (src.startsWith('[') && src.endsWith(']')) return valid(a.getAttribute(src.slice(1, -1)) || '');
    return '';
  };
  U.watch(() => {
    for (const a of document.querySelectorAll(selector)) {
      if (a.tagName !== 'A' || a.__nfSanitized) continue;
      const t = target(a);
      if (t && t !== a.href) { a.setAttribute('href', t); a.__nfSanitized = true; }
    }
  });
}
