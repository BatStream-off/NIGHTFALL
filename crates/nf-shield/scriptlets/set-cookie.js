// set-cookie: set a first-party cookie to a short, harmless value (consent
// answers), only when it does not already hold that value.
function nfSetCookie(name, value, path) {
  if (!name || !/^[\w.-]+$/.test(name)) return;
  const v = String(value === undefined ? '' : value);
  const allowed = /^(true|false|yes|no|y|n|ok|on|off|accept|accepted|reject|rejected|allow|allowed|deny|denied|dismiss|dismissed|necessary|required|hide|hidden|essential|nonessential|checked|unchecked|-?\d{1,15})$/i;
  if (!allowed.test(v) && v !== '') return;
  const current = document.cookie.split(';').map((s) => s.trim()).find((s) => s.startsWith(name + '='));
  if (current && current.slice(name.length + 1) === v) return;
  const p = path === 'none' ? '' : '; path=' + (path && path.startsWith('/') ? path : '/');
  document.cookie = name + '=' + encodeURIComponent(v) + p;
}
