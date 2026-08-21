importScripts("/fp2/realm-common.js");

self.onconnect = (event) => {
  const port = event.ports[0];
  port.onmessage = async (messageEvent) => {
    const data = messageEvent.data;
    if (!data || data.kind !== "fp2-worker-init") return;
    try {
      const result = await FP2Realm.collectWorkerRealm({
        realm: "shared-worker",
        endpoint: data.endpoint,
        nonce: data.nonce,
      });
      port.postMessage({
        kind: "fp2-worker-result",
        nonce: data.nonce,
        result,
      });
    } catch (error) {
      port.postMessage({
        kind: "fp2-worker-result",
        nonce: data.nonce,
        failure: FP2Realm.failureFromError(error, {
          realm: "shared-worker",
          stage: "worker",
          operation: "collectWorkerRealm",
          lastSuccessfulStage: null,
        }),
      });
    }
  };
  port.start();
};
