import { expect, it } from "vitest";
import { previewManagedSilo, previewRuntimeActivation, previewStatus } from "./preview/fixtures.js";
import { activationForSilo, sessionNeedsManagement } from "./runtime-status.js";

it("keeps each Silo's current evidence separate and preserves recovery states", () => {
  const status = previewStatus();
  const otherId = "a2222222-2222-4222-8222-222222222222";
  const other = { ...previewManagedSilo, id: otherId };
  const a = previewRuntimeActivation(previewManagedSilo, "11111111-1111-4111-8111-111111111111");
  const b = previewRuntimeActivation(other, "22222222-2222-4222-8222-222222222222");
  status.activation = null;
  status.sessions = [
    { siloId: previewManagedSilo.id, activation: a },
    { siloId: otherId, activation: b },
  ];

  expect(activationForSilo(status, previewManagedSilo.id).identityEvidence?.siloId).toBe(previewManagedSilo.id);
  expect(activationForSilo(status, otherId).identityEvidence?.siloId).toBe(otherId);
  expect(activationForSilo(status, "missing").identityEvidence).toBeNull();
  expect(sessionNeedsManagement({ siloId: otherId, activation: { ...b, state: "recovery_required", activeSiloId: null } })).toBe(true);
  expect(sessionNeedsManagement({ siloId: otherId, activation: { ...b, state: "failed", activeSiloId: null } })).toBe(false);
  expect(sessionNeedsManagement({ siloId: otherId, activation: { ...b, state: "stopped", activeSiloId: null } })).toBe(false);
});
