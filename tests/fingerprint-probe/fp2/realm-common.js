(function installFp2Realm(global) {
  "use strict";

  const CANVAS_WIDTH = 240;
  const CANVAS_HEIGHT = 120;
  const FONT_UNIVERSE = [
    "Arial",
    "Times New Roman",
    "Courier New",
    "Verdana",
    "Tahoma",
    "Trebuchet MS",
    "Georgia",
    "monospace",
    "serif",
    "sans-serif",
  ];
  const BOGUS_FONTS = [
    "VeriSilo Missing Font 01",
    "VeriSilo Missing Font 02",
    "VeriSilo Missing Font 03",
  ];
  const FAILURE_SCHEMA = "verisilo-fp2-probe-failure/v1";
  const FAILURE_MESSAGE_LIMIT = 256;
  const FAILURE_NAME_LIMIT = 64;
  const FAILURE_SECRET_PATTERN =
    /\b(?:password|passwd|token|secret|authorization|bearer|api[_-]?key|private[_-]?key)\b\s*[:=]\s*[^\s,;)}]+/gi;
  const FAILURE_URL_PATTERN = /\b(?:https?|file):\/\/[^\s,;)}]+/gi;
  const FAILURE_PATH_PATTERN =
    /(?:[A-Za-z]:[\\/][^\s,;)}]+|\\\\[^\s,;)}]+|\/(?:Users|home|tmp|var|private|mnt)\/[^\s,;)}]+)/g;

  function apiShape(apiPresent, reason) {
    return reason ? { apiPresent, reason } : { apiPresent };
  }

  function boundedFailureText(value, limit) {
    return String(value == null ? "" : value)
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, limit);
  }

  function sanitizeFailureMessage(value) {
    let message = boundedFailureText(value, FAILURE_MESSAGE_LIMIT * 2);
    if (!message) return "";
    message = message.replace(FAILURE_SECRET_PATTERN, "<redacted>");
    message = message.replace(FAILURE_URL_PATTERN, "<redacted>");
    message = message.replace(FAILURE_PATH_PATTERN, "<redacted>");
    return boundedFailureText(message, FAILURE_MESSAGE_LIMIT);
  }

  function failureFromError(error, context = {}) {
    const existing =
      error && typeof error === "object" ? error.__fp2Failure : null;
    const source =
      existing && typeof existing === "object"
        ? existing
        : error && typeof error === "object"
          ? error
          : {};
    const fallbackMessage =
      error && typeof error === "object" && "message" in error
        ? error.message
        : error;
    const lastSuccessfulStage = Object.prototype.hasOwnProperty.call(
      source,
      "lastSuccessfulStage",
    )
      ? source.lastSuccessfulStage
      : context.lastSuccessfulStage;
    return {
      schema: FAILURE_SCHEMA,
      realm: boundedFailureText(source.realm || context.realm || "unknown", 96),
      stage: boundedFailureText(source.stage || context.stage || "unknown", 96),
      operation: boundedFailureText(
        source.operation || context.operation || "unknown",
        128,
      ),
      errorName: boundedFailureText(
        source.errorName || (error && error.name) || "Error",
        FAILURE_NAME_LIMIT,
      ),
      errorMessage: sanitizeFailureMessage(
        source.errorMessage !== undefined
          ? source.errorMessage
          : fallbackMessage,
      ),
      lastSuccessfulStage:
        lastSuccessfulStage == null
          ? null
          : boundedFailureText(lastSuccessfulStage, 96),
      probeCompleted: false,
    };
  }

  function failureError(failure) {
    const normalized = failureFromError(failure);
    const error = new Error(normalized.errorMessage || normalized.errorName);
    error.name = normalized.errorName || "Error";
    Object.defineProperty(error, "__fp2Failure", {
      configurable: false,
      enumerable: false,
      value: normalized,
      writable: false,
    });
    return error;
  }

  function createStageTracker(realm) {
    return { realm, lastSuccessfulStage: null };
  }

  function observeStage(tracker, stage, operation, callback) {
    const wrapFailure = (error) => {
      if (error && error.__fp2Failure) return error;
      return failureError(
        failureFromError(error, {
          realm: tracker.realm,
          stage,
          operation,
          lastSuccessfulStage: tracker.lastSuccessfulStage,
        }),
      );
    };
    try {
      const value = callback();
      if (value && typeof value.then === "function") {
        return value.then(
          (result) => {
            tracker.lastSuccessfulStage = stage;
            return result;
          },
          (error) => {
            throw wrapFailure(error);
          },
        );
      }
      tracker.lastSuccessfulStage = stage;
      return value;
    } catch (error) {
      throw wrapFailure(error);
    }
  }

  async function sha256Bytes(bytes) {
    if (!global.crypto || !global.crypto.subtle) {
      throw new Error("crypto_subtle_unavailable");
    }
    const digest = await global.crypto.subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
  }

  async function sha256Text(value) {
    return sha256Bytes(new TextEncoder().encode(String(value)));
  }

  async function withTimeout(promise, milliseconds, label) {
    let timer;
    try {
      return await Promise.race([
        promise,
        new Promise((_, reject) => {
          timer = setTimeout(
            () => reject(new Error(`${label}_timeout`)),
            milliseconds,
          );
        }),
      ]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  function monotonicNow() {
    if (global.performance && typeof global.performance.now === "function") {
      return global.performance.now();
    }
    return Date.now();
  }

  function deadlineFromNow(milliseconds) {
    const duration = Number(milliseconds);
    if (!Number.isFinite(duration) || duration <= 0) {
      const error = new Error("service_worker_activation_deadline_invalid");
      error.name = "ServiceWorkerLifecycleError";
      throw error;
    }
    return monotonicNow() + duration;
  }

  function remainingDeadlineMs(deadline) {
    const remaining = Number(deadline) - monotonicNow();
    return Number.isFinite(remaining) ? Math.max(0, remaining) : 0;
  }

  function serviceWorkerLifecycleError(code, state) {
    const suffix = state == null ? "" : `:${String(state)}`;
    const error = new Error(`${code}${suffix}`);
    error.name =
      code === "service_worker_activation_timeout"
        ? "ServiceWorkerActivationTimeout"
        : "ServiceWorkerLifecycleError";
    return error;
  }

  function waitForServiceWorkerActivation(worker, deadline) {
    if (!worker || typeof worker.state !== "string") {
      throw serviceWorkerLifecycleError("service_worker_active_missing");
    }
    const initialState = worker.state;
    if (initialState === "activated") return Promise.resolve(initialState);
    if (initialState === "redundant") {
      throw serviceWorkerLifecycleError("service_worker_redundant", initialState);
    }
    if (initialState !== "activating") {
      throw serviceWorkerLifecycleError(
        "service_worker_unexpected_state",
        initialState,
      );
    }
    if (
      typeof worker.addEventListener !== "function" ||
      typeof worker.removeEventListener !== "function"
    ) {
      throw serviceWorkerLifecycleError(
        "service_worker_statechange_unavailable",
      );
    }

    const remaining = remainingDeadlineMs(deadline);
    if (remaining <= 0) {
      throw serviceWorkerLifecycleError("service_worker_activation_timeout");
    }

    return new Promise((resolve, reject) => {
      let settled = false;
      let timer;
      const cleanup = () => {
        worker.removeEventListener("statechange", onStateChange);
        if (timer) clearTimeout(timer);
      };
      const finish = (callback, value) => {
        if (settled) return;
        settled = true;
        cleanup();
        callback(value);
      };
      const inspectState = () => {
        const state = worker.state;
        if (state === "activated") {
          finish(resolve, state);
          return;
        }
        if (state === "redundant") {
          finish(
            reject,
            serviceWorkerLifecycleError("service_worker_redundant", state),
          );
          return;
        }
        if (state !== "activating") {
          finish(
            reject,
            serviceWorkerLifecycleError(
              "service_worker_unexpected_state",
              state,
            ),
          );
        }
      };
      function onStateChange() {
        inspectState();
      }

      timer = setTimeout(() => {
        finish(
          reject,
          serviceWorkerLifecycleError(
            "service_worker_activation_timeout",
            worker.state,
          ),
        );
      }, remaining);
      try {
        worker.addEventListener("statechange", onStateChange);
        // The second read closes the statechange-listener installation race.
        inspectState();
      } catch (error) {
        finish(reject, error);
      }
    });
  }

  function normalizeBinaryView(value) {
    if (ArrayBuffer.isView(value)) return Array.from(value);
    if (Array.isArray(value)) return value.map(normalizeBinaryView);
    return value;
  }

  function normalizePngSignature(bytes) {
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    return (
      bytes.length >= signature.length &&
      signature.every((value, index) => bytes[index] === value)
    );
  }

  function dataUrlBytes(dataUrl) {
    const comma = dataUrl.indexOf(",");
    if (comma < 0) throw new Error("data_url_missing_payload");
    const binary = atob(dataUrl.slice(comma + 1));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  }

  function makeWindowCanvas() {
    if (typeof document === "undefined") return null;
    const canvas = document.createElement("canvas");
    canvas.width = CANVAS_WIDTH;
    canvas.height = CANVAS_HEIGHT;
    return canvas;
  }

  function drawFingerprintScene(canvas, fonts) {
    const context = canvas.getContext("2d");
    if (!context) throw new Error("canvas_2d_context_unavailable");
    const fontList = Array.isArray(fonts) && fonts.length ? fonts : ["Arial"];
    const cssFont = fontList
      .map((family) => JSON.stringify(String(family)))
      .join(",");
    context.textBaseline = "alphabetic";
    context.font = `16px ${cssFont}`;
    context.fillStyle = "#e02";
    context.fillRect(10, 10, 60, 40);
    context.strokeStyle = "rgba(0,128,255,0.8)";
    context.lineWidth = 2;
    context.beginPath();
    context.arc(120, 60, 40, 0, Math.PI * 2);
    context.stroke();
    context.fillStyle = "#000";
    context.fillText("VeriSilo FP2 0123456789", 20, 82);
    const gradient = context.createLinearGradient(
      0,
      0,
      CANVAS_WIDTH,
      CANVAS_HEIGHT,
    );
    gradient.addColorStop(0, "#123456");
    gradient.addColorStop(1, "#abcdef");
    context.fillStyle = gradient;
    context.fillRect(160, 20, 70, 40);
    context.save();
    context.shadowColor = "rgba(0,0,0,0.5)";
    context.shadowBlur = 3;
    context.fillStyle = "#0a0";
    context.fillRect(30, 30, 20, 20);
    context.restore();
    return context.getImageData(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT).data;
  }

  async function decodePngBlob(blob) {
    if (typeof global.createImageBitmap !== "function") {
      throw new Error("png_decode_api_unavailable");
    }
    const bitmap = await global.createImageBitmap(blob);
    try {
      if (typeof document !== "undefined") {
        const canvas = makeWindowCanvas();
        const context = canvas.getContext("2d");
        context.drawImage(bitmap, 0, 0);
        const pixels = context.getImageData(
          0,
          0,
          CANVAS_WIDTH,
          CANVAS_HEIGHT,
        ).data;
        return {
          decodedPngPixelsHash: `sha256:${await sha256Bytes(pixels)}`,
          width: bitmap.width,
          height: bitmap.height,
          decodeValid:
            bitmap.width === CANVAS_WIDTH && bitmap.height === CANVAS_HEIGHT,
        };
      }
      const canvas = new OffscreenCanvas(CANVAS_WIDTH, CANVAS_HEIGHT);
      const context = canvas.getContext("2d");
      context.drawImage(bitmap, 0, 0);
      const pixels = context.getImageData(
        0,
        0,
        CANVAS_WIDTH,
        CANVAS_HEIGHT,
      ).data;
      return {
        decodedPngPixelsHash: `sha256:${await sha256Bytes(pixels)}`,
        width: bitmap.width,
        height: bitmap.height,
        decodeValid:
          bitmap.width === CANVAS_WIDTH && bitmap.height === CANVAS_HEIGHT,
      };
    } finally {
      if (typeof bitmap.close === "function") bitmap.close();
    }
  }

  function canvasToBlob(canvas) {
    return new Promise((resolve, reject) => {
      canvas.toBlob(
        (blob) =>
          blob ? resolve(blob) : reject(new Error("canvas_to_blob_null")),
        "image/png",
      );
    });
  }

  async function collectWindowCanvas(fonts) {
    const canvas = makeWindowCanvas();
    if (
      !canvas ||
      typeof canvas.toDataURL !== "function" ||
      typeof canvas.toBlob !== "function"
    ) {
      return {
        apiPresent: false,
        unavailableReason: "window_canvas_export_api_missing",
      };
    }
    const rawPixels = drawFingerprintScene(canvas, fonts);
    const dataUrl = canvas.toDataURL("image/png");
    const dataUrlBytes = dataUrlBytesFromDataUrl(dataUrl);
    const blob = await withTimeout(
      canvasToBlob(canvas),
      3000,
      "window_canvas_blob",
    );
    const pngBytes = new Uint8Array(await blob.arrayBuffer());
    const decoded = await withTimeout(
      decodePngBlob(blob),
      3000,
      "window_png_decode",
    );
    return {
      apiPresent: true,
      rawHash: `sha256:${await sha256Bytes(rawPixels)}`,
      rawRgbaHash: `sha256:${await sha256Bytes(rawPixels)}`,
      decodedPngPixelsHash: decoded.decodedPngPixelsHash,
      pngBytesHash: `sha256:${await sha256Bytes(pngBytes)}`,
      dataUrlHash: `sha256:${await sha256Text(dataUrl)}`,
      exportHash: `sha256:${await sha256Text(dataUrl)}`,
      png: {
        signatureValid: normalizePngSignature(pngBytes),
        dataUrlSignatureValid: normalizePngSignature(dataUrlBytes),
        decodeValid: decoded.decodeValid,
        width: decoded.width,
        height: decoded.height,
        mimeType: blob.type,
      },
    };
  }

  function dataUrlBytesFromDataUrl(value) {
    return dataUrlBytes(value);
  }

  async function collectWorkerCanvas() {
    const offscreenPresent = typeof global.OffscreenCanvas === "function";
    if (!offscreenPresent) {
      return {
        apiPresent: false,
        unavailableReason: "offscreen_canvas_unavailable",
        capabilities: {
          offscreenCanvas: false,
          convertToBlob: false,
          pngDecode: false,
        },
      };
    }
    const canvas = new global.OffscreenCanvas(CANVAS_WIDTH, CANVAS_HEIGHT);
    const context = canvas.getContext("2d");
    const convertToBlobPresent = typeof canvas.convertToBlob === "function";
    const pngDecodePresent = typeof global.createImageBitmap === "function";
    if (!context || !convertToBlobPresent) {
      return {
        apiPresent: false,
        unavailableReason: "offscreen_canvas_export_api_missing",
        capabilities: {
          offscreenCanvas: true,
          convertToBlob: convertToBlobPresent,
          pngDecode: pngDecodePresent,
        },
      };
    }
    const rawPixels = drawFingerprintScene(canvas, ["Arial"]);
    const blob = await withTimeout(
      canvas.convertToBlob({ type: "image/png" }),
      3000,
      "worker_canvas_blob",
    );
    const pngBytes = new Uint8Array(await blob.arrayBuffer());
    if (!pngDecodePresent) {
      return {
        apiPresent: true,
        resultPresent: false,
        unavailableReason: "png_decode_api_unavailable",
        capabilities: {
          offscreenCanvas: true,
          convertToBlob: true,
          pngDecode: false,
        },
        rawHash: `sha256:${await sha256Bytes(rawPixels)}`,
        rawRgbaHash: `sha256:${await sha256Bytes(rawPixels)}`,
        pngBytesHash: `sha256:${await sha256Bytes(pngBytes)}`,
        png: {
          signatureValid: normalizePngSignature(pngBytes),
          decodeValid: false,
          width: null,
          height: null,
          mimeType: blob.type,
        },
      };
    }
    const decoded = await withTimeout(
      decodePngBlob(blob),
      3000,
      "worker_png_decode",
    );
    return {
      apiPresent: true,
      resultPresent: true,
      rawHash: `sha256:${await sha256Bytes(rawPixels)}`,
      rawRgbaHash: `sha256:${await sha256Bytes(rawPixels)}`,
      decodedPngPixelsHash: decoded.decodedPngPixelsHash,
      pngBytesHash: `sha256:${await sha256Bytes(pngBytes)}`,
      dataUrlHash: null,
      exportHash: `sha256:${await sha256Bytes(pngBytes)}`,
      png: {
        signatureValid: normalizePngSignature(pngBytes),
        decodeValid: decoded.decodeValid,
        width: decoded.width,
        height: decoded.height,
        mimeType: blob.type,
      },
      capabilities: {
        offscreenCanvas: true,
        convertToBlob: true,
        pngDecode: true,
      },
    };
  }

  function navigatorSnapshot(navigatorLike) {
    const dntPresent = "doNotTrack" in navigatorLike;
    const gpcPresent = "globalPrivacyControl" in navigatorLike;
    const touchPresent = "maxTouchPoints" in navigatorLike;
    return {
      userAgent: navigatorLike.userAgent,
      platform: navigatorLike.platform,
      hardwareConcurrency: navigatorLike.hardwareConcurrency,
      language: navigatorLike.language,
      languages: Array.from(navigatorLike.languages || []),
      doNotTrack: dntPresent ? navigatorLike.doNotTrack : null,
      globalPrivacyControl: gpcPresent
        ? navigatorLike.globalPrivacyControl
        : null,
      privacySignals: {
        doNotTrack: {
          apiPresent: dntPresent,
          value: dntPresent ? navigatorLike.doNotTrack : null,
        },
        globalPrivacyControl: {
          apiPresent: gpcPresent,
          value: gpcPresent ? navigatorLike.globalPrivacyControl : null,
        },
      },
      maxTouchPoints: {
        apiPresent: touchPresent,
        value: touchPresent ? navigatorLike.maxTouchPoints : null,
      },
    };
  }

  function localeSnapshot() {
    const options = new Intl.DateTimeFormat().resolvedOptions();
    return {
      timeZone: options.timeZone || null,
      utcOffsetMinutes: new Date().getTimezoneOffset(),
    };
  }

  function webglSnapshot(contextName) {
    if (typeof document === "undefined") {
      return { apiPresent: false, unavailableReason: "document_unavailable" };
    }
    const canvas = document.createElement("canvas");
    const context = canvas.getContext(contextName, {
      preserveDrawingBuffer: true,
    });
    if (!context) {
      return {
        apiPresent: false,
        unavailableReason: `${contextName}_context_unavailable`,
      };
    }
    const debugInfo = context.getExtension("WEBGL_debug_renderer_info");
    const parameterNames = [
      "VERSION",
      "SHADING_LANGUAGE_VERSION",
      "MAX_TEXTURE_SIZE",
      "MAX_CUBE_MAP_TEXTURE_SIZE",
      "MAX_VERTEX_ATTRIBS",
      "MAX_VIEWPORT_DIMS",
    ];
    const parameters = {};
    for (const name of parameterNames) {
      const constant = context[name];
      try {
        parameters[name] =
          constant === undefined
            ? null
            : normalizeBinaryView(context.getParameter(constant));
      } catch (_error) {
        parameters[name] = null;
      }
    }
    return {
      apiPresent: true,
      vendor: debugInfo
        ? context.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL)
        : context.getParameter(context.VENDOR),
      renderer: debugInfo
        ? context.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL)
        : context.getParameter(context.RENDERER),
      supportedExtensions: Array.from(
        context.getSupportedExtensions() || [],
      ).sort(),
      parameters,
    };
  }

  function workerWebglSnapshot(contextName) {
    if (typeof global.OffscreenCanvas !== "function") {
      return {
        apiPresent: false,
        unavailableReason: "offscreen_canvas_unavailable",
      };
    }
    const canvas = new global.OffscreenCanvas(2, 2);
    const context = canvas.getContext(contextName);
    if (!context) {
      return {
        apiPresent: false,
        unavailableReason: `${contextName}_context_unavailable`,
      };
    }
    return {
      apiPresent: true,
      vendor: context.getParameter(context.VENDOR),
      renderer: context.getParameter(context.RENDERER),
      supportedExtensions: Array.from(
        context.getSupportedExtensions() || [],
      ).sort(),
      parameters: {
        VERSION: context.getParameter(context.VERSION),
        SHADING_LANGUAGE_VERSION: context.getParameter(
          context.SHADING_LANGUAGE_VERSION,
        ),
        MAX_TEXTURE_SIZE: context.getParameter(context.MAX_TEXTURE_SIZE),
      },
    };
  }

  function screenSnapshot() {
    return {
      width: screen.width,
      height: screen.height,
      availWidth: screen.availWidth,
      availHeight: screen.availHeight,
      availTop: screen.availTop,
      availLeft: screen.availLeft,
      colorDepth: screen.colorDepth,
      pixelDepth: screen.pixelDepth,
    };
  }

  function geometrySnapshot() {
    return {
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      outerWidth: window.outerWidth,
      outerHeight: window.outerHeight,
      screenX: window.screenX,
      screenY: window.screenY,
      screenLeft: window.screenLeft,
      screenTop: window.screenTop,
    };
  }

  function fontSnapshot(fonts) {
    const fontApiPresent = typeof document !== "undefined" && !!document.fonts;
    if (!fontApiPresent) {
      return {
        apiPresent: false,
        unavailableReason: "document_fonts_unavailable",
      };
    }
    const families = Array.isArray(fonts) ? fonts.map(String) : [];
    const injectedFonts = families.map((family) => ({
      family,
      available: document.fonts.check(
        `16px "${family.replaceAll('"', '\\"')}"`,
      ),
    }));
    const fontNegativeControls = Object.fromEntries(
      BOGUS_FONTS.map((family) => [
        family,
        document.fonts.check(`16px "${family}"`),
      ]),
    );
    const canvas = makeWindowCanvas();
    const context = canvas.getContext("2d");
    const fontUniverseWidths = Object.fromEntries(
      FONT_UNIVERSE.map((family) => {
        context.font = `16px "${family}"`;
        return [family, context.measureText("VeriSilo FP2 0123456789").width];
      }),
    );
    return {
      apiPresent: true,
      injectedFonts,
      fontNegativeControls,
      fontUniverseWidths,
    };
  }

  async function voiceSnapshot() {
    if (
      typeof global.speechSynthesis === "undefined" ||
      typeof global.speechSynthesis.getVoices !== "function"
    ) {
      return {
        apiPresent: false,
        unavailableReason: "speech_synthesis_unavailable",
      };
    }
    let voices = global.speechSynthesis.getVoices();
    if (!voices.length) {
      await withTimeout(
        new Promise((resolve) => {
          const finish = () => {
            global.speechSynthesis.removeEventListener("voiceschanged", finish);
            resolve();
          };
          global.speechSynthesis.addEventListener("voiceschanged", finish, {
            once: true,
          });
        }),
        3000,
        "voices_ready",
      ).catch(() => undefined);
      voices = global.speechSynthesis.getVoices();
    }
    return {
      apiPresent: true,
      voices: voices.map((voice) => ({
        name: voice.name,
        lang: voice.lang,
        localService: voice.localService,
        voiceURI: voice.voiceURI,
        isDefault: voice.default,
      })),
    };
  }

  async function mediaSnapshot() {
    if (
      typeof navigator.mediaDevices === "undefined" ||
      typeof navigator.mediaDevices.enumerateDevices !== "function"
    ) {
      return {
        apiPresent: false,
        unavailableReason: "media_devices_unavailable",
      };
    }
    const devices = await withTimeout(
      navigator.mediaDevices.enumerateDevices(),
      3000,
      "media_devices",
    );
    const counts = { audioinput: 0, videoinput: 0, audiooutput: 0 };
    for (const device of devices) {
      if (Object.prototype.hasOwnProperty.call(counts, device.kind))
        counts[device.kind] += 1;
    }
    return {
      apiPresent: true,
      counts,
      deviceKinds: devices.map((device) => device.kind).sort(),
    };
  }

  async function audioSnapshot() {
    if (typeof global.OfflineAudioContext !== "function") {
      return {
        apiPresent: false,
        unavailableReason: "offline_audio_context_unavailable",
      };
    }
    const sampleRate = 48000;
    const length = Math.floor(sampleRate * 0.15);
    const context = new global.OfflineAudioContext(1, length, sampleRate);
    const buffer = context.createBuffer(1, length, sampleRate);
    const channel = buffer.getChannelData(0);
    for (let index = 0; index < channel.length; index += 1) {
      channel[index] =
        0.2 * Math.sin((2 * Math.PI * 440 * index) / sampleRate) +
        0.1 * Math.sin((2 * Math.PI * 880 * index) / sampleRate);
    }
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    source.start();
    const rendered = await withTimeout(
      context.startRendering(),
      3000,
      "audio_render",
    );
    const renderedChannel = rendered.getChannelData(0);
    return {
      apiPresent: true,
      audioHash: `sha256:${await sha256Bytes(
        new Uint8Array(
          renderedChannel.buffer,
          renderedChannel.byteOffset,
          renderedChannel.byteLength,
        ),
      )}`,
    };
  }

  async function observeHeaders(endpoint, realm, nonce) {
    const url = `${endpoint}?realm=${encodeURIComponent(realm)}&nonce=${encodeURIComponent(nonce)}`;
    const response = await withTimeout(
      global.fetch(url, {
        method: "GET",
        cache: "no-store",
        credentials: "omit",
        headers: {
          "X-FP2-Realm": realm,
          "X-FP2-Nonce": nonce,
        },
      }),
      3000,
      `${realm}_header_fetch`,
    );
    if (!response.ok)
      throw new Error(`${realm}_header_http_${response.status}`);
    const body = await response.json();
    if (!body || body.ok !== true || body.realm !== realm) {
      throw new Error(`${realm}_header_protocol_mismatch`);
    }
    return {
      identityHeaders: body.identityHeaders,
      contextHeaders: body.contextHeaders,
      requestPolicy: {
        method: "GET",
        cache: "no-store",
        credentials: "omit",
      },
    };
  }

  async function collectWindowRealm({ realm, endpoint, nonce, fonts }) {
    const tracker = createStageTracker(realm);
    const navigatorValue = observeStage(
      tracker,
      "navigator",
      "navigatorSnapshot",
      () => navigatorSnapshot(global.navigator),
    );
    const locale = observeStage(
      tracker,
      "localeTimezone",
      "localeSnapshot",
      () => localeSnapshot(),
    );
    const canvas = await observeStage(tracker, "canvas", "identityCanvas", () =>
      collectWindowCanvas(fonts),
    );
    const audio = await observeStage(tracker, "audio", "audioSnapshot", () =>
      audioSnapshot(),
    );
    const webgl = observeStage(tracker, "webgl", "webglSnapshot", () =>
      webglSnapshot("webgl"),
    );
    const webgl2 = observeStage(tracker, "webgl2", "webgl2Snapshot", () =>
      webglSnapshot("webgl2"),
    );
    const fontsResult = observeStage(tracker, "fonts", "fontSnapshot", () =>
      fontSnapshot(fonts),
    );
    const voices = await observeStage(tracker, "voices", "voiceSnapshot", () =>
      voiceSnapshot(),
    );
    const mediaDevices = await observeStage(
      tracker,
      "media",
      "mediaSnapshot",
      () => mediaSnapshot(),
    );
    const requestHeaders = await observeStage(
      tracker,
      "headerRequest",
      "observeHeaders",
      () => observeHeaders(endpoint, realm, nonce),
    );
    const screenValue = observeStage(tracker, "screen", "screenSnapshot", () =>
      screenSnapshot(),
    );
    const devicePixelRatio = observeStage(
      tracker,
      "screen",
      "devicePixelRatio",
      () => window.devicePixelRatio,
    );
    const geometry = observeStage(tracker, "geometry", "geometrySnapshot", () =>
      geometrySnapshot(),
    );
    const historyLength = observeStage(
      tracker,
      "history",
      "history.length",
      () => history.length,
    );
    return observeStage(tracker, "finalize", "windowRealmResult", () => ({
      realm,
      kind: "window",
      verified: false,
      navigator: {
        userAgent: navigatorValue.userAgent,
        platform: navigatorValue.platform,
        hardwareConcurrency: navigatorValue.hardwareConcurrency,
        language: navigatorValue.language,
        languages: navigatorValue.languages,
        doNotTrack: navigatorValue.doNotTrack,
        globalPrivacyControl: navigatorValue.globalPrivacyControl,
      },
      locale,
      screen: screenValue,
      devicePixelRatio,
      geometry,
      historyLength,
      canvas,
      audio,
      webgl,
      webgl2,
      fonts: fontsResult,
      voices,
      mediaDevices,
      privacySignals: navigatorValue.privacySignals,
      maxTouchPoints: navigatorValue.maxTouchPoints,
      requestHeaders,
      capabilities: {
        navigator: apiShape(true),
        localeTimezone: apiShape(true),
        screenDpr: apiShape(
          typeof screen !== "undefined" &&
            typeof window.devicePixelRatio === "number",
        ),
        geometry: apiShape(true),
        history: apiShape(typeof history !== "undefined"),
        canvas: apiShape(canvas.apiPresent, canvas.unavailableReason),
        audio: apiShape(audio.apiPresent, audio.unavailableReason),
        webgl: apiShape(webgl.apiPresent, webgl.unavailableReason),
        webgl2: apiShape(webgl2.apiPresent, webgl2.unavailableReason),
        fonts: apiShape(fontsResult.apiPresent, fontsResult.unavailableReason),
        voices: apiShape(voices.apiPresent, voices.unavailableReason),
        mediaDevices: apiShape(
          mediaDevices.apiPresent,
          mediaDevices.unavailableReason,
        ),
        privacySignals: {
          doNotTrack: navigatorValue.privacySignals.doNotTrack,
          globalPrivacyControl:
            navigatorValue.privacySignals.globalPrivacyControl,
        },
        maxTouchPoints: navigatorValue.maxTouchPoints,
        httpHeaders: apiShape(true),
      },
    }));
  }

  async function collectWorkerRealm({ realm, endpoint, nonce }) {
    const tracker = createStageTracker(realm);
    const navigatorValue = observeStage(
      tracker,
      "navigator",
      "navigatorSnapshot",
      () => navigatorSnapshot(global.navigator),
    );
    const workerCanvas = await observeStage(
      tracker,
      "canvas",
      "workerCanvas",
      () => collectWorkerCanvas(),
    );
    const webgl = observeStage(tracker, "webgl", "workerWebglSnapshot", () =>
      workerWebglSnapshot("webgl"),
    );
    const webgl2 = observeStage(tracker, "webgl2", "workerWebglSnapshot", () =>
      workerWebglSnapshot("webgl2"),
    );
    const requestHeaders = await observeStage(
      tracker,
      "headerRequest",
      "observeHeaders",
      () => observeHeaders(endpoint, realm, nonce),
    );
    const fontApiPresent = observeStage(
      tracker,
      "fonts",
      "workerFontApi",
      () => typeof global.queryLocalFonts === "function",
    );
    const locale = observeStage(
      tracker,
      "localeTimezone",
      "localeSnapshot",
      () => localeSnapshot(),
    );
    return observeStage(tracker, "finalize", "workerRealmResult", () => ({
      realm,
      kind: "worker",
      verified: false,
      navigator: {
        userAgent: navigatorValue.userAgent,
        platform: navigatorValue.platform,
        hardwareConcurrency: navigatorValue.hardwareConcurrency,
        language: navigatorValue.language,
        languages: navigatorValue.languages,
        doNotTrack: navigatorValue.doNotTrack,
        globalPrivacyControl: navigatorValue.globalPrivacyControl,
      },
      locale,
      workerCanvas,
      webgl,
      webgl2,
      fonts: {
        apiPresent: fontApiPresent,
        unavailableReason: fontApiPresent
          ? undefined
          : "worker_font_api_unavailable",
      },
      privacySignals: navigatorValue.privacySignals,
      maxTouchPoints: navigatorValue.maxTouchPoints,
      requestHeaders,
      capabilities: {
        navigator: apiShape(true),
        localeTimezone: apiShape(true),
        webgl: apiShape(webgl.apiPresent, webgl.unavailableReason),
        webgl2: apiShape(webgl2.apiPresent, webgl2.unavailableReason),
        fonts: apiShape(
          fontApiPresent,
          fontApiPresent ? undefined : "worker_font_api_unavailable",
        ),
        privacySignals: {
          doNotTrack: navigatorValue.privacySignals.doNotTrack,
          globalPrivacyControl:
            navigatorValue.privacySignals.globalPrivacyControl,
        },
        maxTouchPoints: navigatorValue.maxTouchPoints,
        httpHeaders: apiShape(true),
        workerCanvas: {
          apiPresent: workerCanvas.apiPresent,
          reason: workerCanvas.unavailableReason,
          capabilities: workerCanvas.capabilities || null,
        },
      },
    }));
  }

  global.FP2Realm = {
    CANVAS_WIDTH,
    CANVAS_HEIGHT,
    FONT_UNIVERSE,
    BOGUS_FONTS,
    FAILURE_SCHEMA,
    sha256Bytes,
    sha256Text,
    failureFromError,
    failureError,
    createStageTracker,
    observeStage,
    collectWindowRealm,
    collectWorkerRealm,
    observeHeaders,
    withTimeout,
    deadlineFromNow,
    remainingDeadlineMs,
    waitForServiceWorkerActivation,
  };
})(globalThis);
