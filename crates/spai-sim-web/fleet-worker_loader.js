// Starts the fleet worker; the page's build hash comes along in the query, so a cached worker of
// another build is never paired with this page.
importScripts("./fleet-worker.js" + self.location.search);
wasm_bindgen("./fleet-worker_bg.wasm" + self.location.search);
