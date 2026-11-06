// set-session-storage-item: same as set-local-storage-item, for sessionStorage.
function nfSetSessionStorageItem(key, value) {
  nfUtils().storageItem(window.sessionStorage, key, value);
}
