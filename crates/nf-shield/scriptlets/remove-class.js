// remove-class (rc): remove classes from matching elements, continuously.
function nfRemoveClass(classes, selector) {
  const U = nfUtils();
  const list = (classes || '').split('|').filter(Boolean);
  if (!list.length) return;
  const sel = selector || list.map((c) => '.' + CSS.escape(c)).join(',');
  U.watch(() => { for (const el of document.querySelectorAll(sel)) el.classList.remove(...list); });
}
