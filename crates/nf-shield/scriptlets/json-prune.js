// json-prune: remove properties from objects produced by JSON.parse and
// Response.json().
function nfJsonPrune(prunePaths, needlePaths) {
  const U = nfUtils();
  if (!prunePaths) return;
  const origParse = JSON.parse;
  JSON.parse = U.cloak(function (...args) {
    const r = origParse.apply(this, args);
    U.prune(r, prunePaths, needlePaths);
    return r;
  }, origParse);
  const origJson = Response.prototype.json;
  Response.prototype.json = U.cloak(function (...args) {
    return origJson.apply(this, args).then((r) => { U.prune(r, prunePaths, needlePaths); return r; });
  }, origJson);
}
