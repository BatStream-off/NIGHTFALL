// remove-attr (ra): remove attributes from matching elements, continuously.
function nfRemoveAttr(attrs, selector) {
  const U = nfUtils();
  const list = (attrs || '').split('|').filter(Boolean);
  if (!list.length) return;
  const sel = selector || list.map((a) => '[' + CSS.escape(a) + ']').join(',');
  U.watch(() => { for (const el of document.querySelectorAll(sel)) for (const a of list) el.removeAttribute(a); });
}
