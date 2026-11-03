// abort-on-property-write (aopw): throw when the page writes a property.
function nfAbortOnPropertyWrite(prop) {
  if (!prop || prop.includes('.')) return;
  try {
    Object.defineProperty(window, prop, { configurable: false, get() { return undefined; }, set() { throw new ReferenceError('nightfall'); } });
  } catch (e) {}
}
