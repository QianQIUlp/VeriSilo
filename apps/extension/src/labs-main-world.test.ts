import { afterEach, describe, expect, it, vi } from "vitest";

import {
  installDedicatedWorkerExperiment,
  restoreDedicatedWorkerExperiment,
} from "./labs-main-world.js";

const SITE_ORIGIN = "https://example.test";

type FakeDocument = {
  defaultView: FakeRealm;
  documentElement: object;
  querySelectorAll: () => Array<{ contentWindow: FakeRealm }>;
};

type FakeRealm = {
  location: { origin: string; href: string };
  parent: FakeRealm;
  frameElement: { ownerDocument: FakeDocument } | null;
  document: FakeDocument;
  Worker: typeof Worker;
  URL: typeof URL;
  Blob: typeof Blob;
  addEventListener: () => void;
  removeEventListener: () => void;
  clearTimeout: () => void;
  setTimeout: () => number;
  postMessage: () => void;
};

function nativeWorker(): typeof Worker {
  return function Worker() {} as unknown as typeof Worker;
}

function realm(origin: string, href: string): FakeRealm {
  const value = {
    location: { origin, href },
    parent: undefined,
    frameElement: null,
    document: undefined,
    Worker: nativeWorker(),
    URL,
    Blob,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    clearTimeout: vi.fn(),
    setTimeout: vi.fn(() => 1),
    postMessage: vi.fn(),
  } as unknown as FakeRealm;
  value.parent = value;
  value.document = {
    defaultView: value,
    documentElement: {},
    querySelectorAll: () => [],
  };
  return value;
}

function attachFrame(parent: FakeRealm, child: FakeRealm): void {
  child.parent = parent;
  child.frameElement = { ownerDocument: parent.document };
  parent.document.querySelectorAll = () => [{ contentWindow: child }];
}

function install(root: FakeRealm) {
  vi.stubGlobal("window", root);
  vi.stubGlobal("document", root.document);
  vi.stubGlobal("location", root.location);
  vi.stubGlobal("Worker", root.Worker);
  vi.stubGlobal(
    "MutationObserver",
    class {
      disconnect(): void {}
      observe(): void {}
    },
  );
  return installDedicatedWorkerExperiment({
    runId: "test-run",
    siteOrigin: SITE_ORIGIN,
    canary: "test-canary",
    expiresAtUnixMs: Date.now() + 60_000,
  });
}

describe("Dedicated Worker inherited iframe realms", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it.each(["about:blank", "about:srcdoc"])(
    "wraps an accessible null-origin %s realm inherited from the site",
    (href) => {
      const root = realm(SITE_ORIGIN, `${SITE_ORIGIN}/page`);
      const inherited = realm("null", href);
      const inheritedNativeWorker = inherited.Worker;
      attachFrame(root, inherited);

      expect(install(root)).toMatchObject({
        active: true,
        constructorWrapped: true,
        wrappedRealmCount: 2,
      });
      expect(inherited.Worker).not.toBe(inheritedNativeWorker);

      expect(restoreDedicatedWorkerExperiment({ runId: "test-run" })).toEqual({
        restored: true,
        constructorNative: true,
      });
      expect(inherited.Worker).toBe(inheritedNativeWorker);
    },
  );

  it("does not treat every accessible null-origin iframe as site-owned", () => {
    const root = realm(SITE_ORIGIN, `${SITE_ORIGIN}/page`);
    const opaque = realm("null", "data:text/html,opaque");
    const opaqueNativeWorker = opaque.Worker;
    attachFrame(root, opaque);

    expect(install(root)).toMatchObject({ wrappedRealmCount: 1 });
    expect(opaque.Worker).toBe(opaqueNativeWorker);
  });

  it("requires inherited about documents to lead back to the exact site origin", () => {
    const root = realm(SITE_ORIGIN, `${SITE_ORIGIN}/page`);
    const otherParent = realm("https://other.test", "https://other.test/page");
    const inherited = realm("null", "about:blank");
    const inheritedNativeWorker = inherited.Worker;
    attachFrame(otherParent, inherited);
    root.document.querySelectorAll = () => [{ contentWindow: inherited }];

    expect(install(root)).toMatchObject({ wrappedRealmCount: 1 });
    expect(inherited.Worker).toBe(inheritedNativeWorker);
  });
});
