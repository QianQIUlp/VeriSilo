const observation = {
  origin: location.origin,
  navigator: {
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    languages: [...navigator.languages],
  },
};

window.postMessage(
  { source: "verisilo-main-world", observation },
  location.origin,
);
