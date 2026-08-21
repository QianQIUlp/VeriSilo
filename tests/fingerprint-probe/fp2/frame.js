(function installFp2Frame() {
  "use strict";

  const query = new URLSearchParams(window.location.search);
  const role = query.get("role") || "";
  const nonce = query.get("nonce") || "";

  window.addEventListener("message", async (event) => {
    const data = event.data;
    if (!data || data.kind !== "fp2-frame-init") return;
    if (event.source !== window.parent || event.origin !== data.parentOrigin)
      return;
    if (data.nonce !== nonce || !Array.isArray(data.input && data.input.fonts))
      return;
    try {
      const result = await FP2Realm.collectWindowRealm({
        realm: role,
        endpoint: `${window.location.origin}/fp2/header-observation`,
        nonce,
        fonts: data.input.fonts,
      });
      window.parent.postMessage(
        {
          kind: "fp2-frame-result",
          realm: role,
          nonce,
          result,
        },
        data.parentOrigin,
      );
    } catch (error) {
      window.parent.postMessage(
        {
          kind: "fp2-frame-result",
          realm: role,
          nonce,
          failure: FP2Realm.failureFromError(error, {
            realm: role,
            stage: "frame",
            operation: "collectWindowRealm",
            lastSuccessfulStage: null,
          }),
        },
        data.parentOrigin,
      );
    }
  });
})();
