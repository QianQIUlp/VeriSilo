(function reportServiceWorkerControl() {
  "use strict";

  const nonce = new URLSearchParams(window.location.search).get("nonce") || "";
  const report = () => {
    if (!navigator.serviceWorker.controller) return false;
    window.parent.postMessage(
      { kind: "fp2-controlled-page", nonce, controlled: true },
      window.location.origin,
    );
    return true;
  };
  if (!report() && navigator.serviceWorker) {
    navigator.serviceWorker.addEventListener("controllerchange", report, {
      once: true,
    });
    navigator.serviceWorker.ready.then(report).catch(() => undefined);
  }
})();
