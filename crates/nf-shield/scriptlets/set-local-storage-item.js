// set-local-storage-item: give a localStorage entry a fixed, harmless value
// (typically "consent given" or "already seen"), or remove it ($remove$).
function nfSetLocalStorageItem(key, value) {
  nfUtils().storageItem(window.localStorage, key, value);
}
