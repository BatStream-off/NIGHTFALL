// noeval-if: block matching eval() calls.
function nfNoEvalIf(needle) {
  const U = nfUtils();
  const re = U.re(needle);
  const orig = window.eval;
  window.eval = U.cloak(function (code) {
    if (re.test(String(code))) return;
    return orig.call(window, code);
  }, orig);
}
