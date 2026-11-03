// remove-node-text (rmnt): empty matching text/script nodes.
function nfRemoveNodeText(nodeName, needle) {
  const U = nfUtils();
  if (!nodeName) return;
  const nameRe = U.re(nodeName.toLowerCase());
  const re = U.re(needle);
  const check = (node) => {
    if (nameRe.test(node.nodeName.toLowerCase()) && re.test(node.textContent || '')) node.textContent = '';
  };
  new MutationObserver((ms) => { for (const m of ms) for (const n of m.addedNodes) check(n); })
    .observe(document, { childList: true, subtree: true });
}
