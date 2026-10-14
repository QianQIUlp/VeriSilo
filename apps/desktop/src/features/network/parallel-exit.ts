import type { NetworkProfile, RuntimeState, Silo } from "@verisilo/contracts";

import type {
  RuntimeSessionStatus,
  SiloNetworkEvidence,
} from "../../desktop-api.js";

/**
 * 并行同出口提醒（Parallel same-exit caution）。
 *
 * 并发 v1 允许最多两个本地 Managed 会话同时运行。两个长期身份在同一个
 * 网络出口上并行时，网站有机会通过出口 IP 把它们关联起来。这里只计算
 * 「共用出口」这一事实并交给 UI 非阻塞地提醒；它不阻止运行、不断言
 * 身份一定被关联，也不把配置当成观测。
 *
 * 两种依据严格分开：
 * - `configured`：按当前 Silo 配置即可静态判定的共用出口（都是直连，
 *   或同一代理入口）。它描述配置，不描述实际出口。
 * - `observed`：两个会话的 Silo 各自有一条未过期的出口检查记录，且
 *   记录的出口地址相同。它来自历史检查，不是当前运行的实时验证。
 * 同一对 Silo 只产出一条依据：能观测时优先观测，否则回退配置。
 */

export type ParallelExitBasis = "configured" | "observed";

export interface ParallelExitFinding {
  basis: ParallelExitBasis;
  peerSiloId: string;
  peerName: string;
  /** Human-readable shared exit, e.g. "直连（本机网络）" or "proxy.example.test:3128". */
  sharedExitLabel: string;
  /** For `observed`: when the shared-address check was received. */
  observedAt?: string;
}

/** Session states in which the browser is or will soon be carrying traffic. */
const trafficCarryingStates: ReadonlySet<RuntimeState> = new Set([
  "preflight",
  "launching",
  "running",
]);

function isTrafficCarrying(session: RuntimeSessionStatus): boolean {
  return trafficCarryingStates.has(session.activation.state);
}

/**
 * Normalized configured-exit key, or `null` when the configured exit cannot
 * be compared statically (PAC routes per rule). Proxy keys keep the scheme so
 * different entry protocols on the same host stay conservative (fewer
 * configured findings, never more).
 */
export function configuredExitKey(profile: NetworkProfile): string | null {
  if (profile.mode === "direct") {
    return "direct";
  }
  if (profile.mode === "fixed_proxy") {
    return `proxy:${profile.scheme}:${profile.host.toLowerCase()}:${profile.port}`;
  }
  return null;
}

/** Display label for a statically comparable configured exit. */
export function configuredExitLabel(profile: NetworkProfile): string {
  if (profile.mode === "direct") {
    return "直连（本机网络）";
  }
  if (profile.mode === "fixed_proxy") {
    return `${profile.host}:${profile.port}`;
  }
  return "PAC 规则";
}

/** Exit addresses are compared case-insensitively after trimming. */
export function normalizeExitAddress(address: string): string {
  return address.trim().toLowerCase();
}

/**
 * Latest unexpired exit-check record per Silo. Records without an exit
 * address observation never qualify.
 */
export function latestUnexpiredEvidenceBySilo(
  evidence: SiloNetworkEvidence[],
  now: number,
): Map<string, SiloNetworkEvidence> {
  const latest = new Map<string, SiloNetworkEvidence>();
  for (const entry of evidence) {
    if (entry.result.ip === null) {
      continue;
    }
    if (Date.parse(entry.expiresAt) <= now) {
      continue;
    }
    const current = latest.get(entry.siloId);
    if (
      current === undefined ||
      Date.parse(entry.receivedAt) > Date.parse(current.receivedAt)
    ) {
      latest.set(entry.siloId, entry);
    }
  }
  return latest;
}

function sharedObservedFinding(
  silo: Silo,
  peer: Silo,
  evidenceBySilo: Map<string, SiloNetworkEvidence>,
): ParallelExitFinding | null {
  const own = evidenceBySilo.get(silo.id);
  const peerEvidence = evidenceBySilo.get(peer.id);
  if (own === undefined || peerEvidence === undefined) {
    return null;
  }
  const ownAddress = own.result.ip?.address;
  const peerAddress = peerEvidence.result.ip?.address;
  if (
    ownAddress === undefined ||
    peerAddress === undefined ||
    normalizeExitAddress(ownAddress) !== normalizeExitAddress(peerAddress)
  ) {
    return null;
  }
  return {
    basis: "observed",
    peerSiloId: peer.id,
    peerName: peer.name,
    sharedExitLabel: ownAddress.trim(),
    observedAt: own.receivedAt,
  };
}

function sharedConfiguredFinding(
  silo: Silo,
  peer: Silo,
): ParallelExitFinding | null {
  const ownKey = configuredExitKey(silo.networkProfile);
  if (ownKey === null || ownKey !== configuredExitKey(peer.networkProfile)) {
    return null;
  }
  return {
    basis: "configured",
    peerSiloId: peer.id,
    peerName: peer.name,
    sharedExitLabel: configuredExitLabel(silo.networkProfile),
  };
}

function peerIsComparableLocalSession(peer: Silo | undefined): peer is Silo {
  return peer !== undefined && peer.executionTarget.kind === "local";
}

export interface ParallelExitInput {
  silos: Silo[];
  sessions: RuntimeSessionStatus[];
  networkEvidence: SiloNetworkEvidence[];
  now: number;
}

/**
 * For every running local Managed Camoufox session, find the concurrently
 * traffic-carrying local sessions it shares an exit with. Managed identities
 * are the caution target (the concurrency audience with the highest
 * accidental-correlation cost); peers may be any local Silo. Returns one
 * entry per cautioned Silo, each with at most one finding per peer.
 */
export function collectParallelExitFindings(
  input: ParallelExitInput,
): Map<string, ParallelExitFinding[]> {
  const findings = new Map<string, ParallelExitFinding[]>();
  const siloById = new Map(input.silos.map((silo) => [silo.id, silo] as const));
  const running = input.sessions.filter(isTrafficCarrying);
  if (running.length < 2) {
    return findings;
  }
  const evidenceBySilo = latestUnexpiredEvidenceBySilo(
    input.networkEvidence,
    input.now,
  );
  for (const session of running) {
    const silo = siloById.get(session.siloId);
    if (
      silo === undefined ||
      silo.executionTarget.kind !== "local" ||
      silo.engine.adapter !== "camoufox"
    ) {
      continue;
    }
    const siloFindings: ParallelExitFinding[] = [];
    for (const peerSession of running) {
      if (peerSession.siloId === session.siloId) {
        continue;
      }
      const peer = siloById.get(peerSession.siloId);
      if (!peerIsComparableLocalSession(peer)) {
        continue;
      }
      const finding =
        sharedObservedFinding(silo, peer, evidenceBySilo) ??
        sharedConfiguredFinding(silo, peer);
      if (finding !== null) {
        siloFindings.push(finding);
      }
    }
    if (siloFindings.length > 0) {
      findings.set(session.siloId, siloFindings);
    }
  }
  return findings;
}

/**
 * Pre-launch hint for a local Managed Silo that is not running yet: would
 * opening it now create a configured shared exit with a session that is
 * already carrying traffic? Configured only — a run that has not started has
 * no observations to compare, and old records must not describe the new run.
 */
export function parallelExitLaunchHint(
  silo: Silo,
  silos: Silo[],
  sessions: RuntimeSessionStatus[],
): ParallelExitFinding | null {
  if (
    silo.executionTarget.kind !== "local" ||
    silo.engine.adapter !== "camoufox"
  ) {
    return null;
  }
  const alreadyRunning = sessions.some(
    (session) => session.siloId === silo.id && isTrafficCarrying(session),
  );
  if (alreadyRunning) {
    return null;
  }
  const siloById = new Map(silos.map((entry) => [entry.id, entry] as const));
  for (const peerSession of sessions) {
    if (peerSession.siloId === silo.id || !isTrafficCarrying(peerSession)) {
      continue;
    }
    const peer = siloById.get(peerSession.siloId);
    if (!peerIsComparableLocalSession(peer)) {
      continue;
    }
    const finding = sharedConfiguredFinding(silo, peer);
    if (finding !== null) {
      return finding;
    }
  }
  return null;
}
