importScripts("/fp2/realm-common.js");

self.onmessage = async (event) => {
  const data = event.data;
  if (!data || data.kind !== "fp2-worker-init") return;
  try {
    const result = await FP2Realm.collectWorkerRealm({
      realm: "dedicated-worker",
      endpoint: data.endpoint,
      nonce: data.nonce,
    });
    self.postMessage({ kind: "fp2-worker-result", nonce: data.nonce, result });
  } catch (error) {
    self.postMessage({
      kind: "fp2-worker-result",
      nonce: data.nonce,
      failure: FP2Realm.failureFromError(error, {
        realm: "dedicated-worker",
        stage: "worker",
        operation: "collectWorkerRealm",
        lastSuccessfulStage: null,
      }),
    });
  }
};
