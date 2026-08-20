importScripts("/fp2/realm-common.js");

self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("message", (event) => {
  const data = event.data;
  const port = event.ports && event.ports[0];
  if (!data || data.kind !== "fp2-service-worker-init" || !port) return;
  Promise.resolve()
    .then(() =>
      FP2Realm.collectWorkerRealm({
        realm: "service-worker",
        endpoint: data.endpoint,
        nonce: data.nonce,
      }),
    )
    .then((result) => {
      port.postMessage({
        kind: "fp2-service-worker-result",
        nonce: data.nonce,
        result,
      });
    })
    .catch((error) => {
      port.postMessage({
        kind: "fp2-service-worker-result",
        nonce: data.nonce,
        error: error && error.name ? error.name : "Error",
      });
    });
});
