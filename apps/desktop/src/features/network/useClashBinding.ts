import { useCallback, useEffect, useRef, useState } from "react";

import {
  desktopApi,
  type LocalClashProbe,
  type MihomoSelectorGroup,
  type MihomoSnapshot,
} from "../../desktop-api.js";

import { errorNotice } from "../../shared/notice.js";

import { UserFacingError } from "../../user-errors.js";

import { clashControllerLabel } from "../../proxy-presets.js";

import { readMihomoGroups } from "./controller.js";

/**
 * The binding a Silo keeps to a local Clash/Mihomo controller: the control URL
 * used to read groups plus the group and node every launch re-selects.
 */
export interface ClashBindingSelection {
  controllerUrl: string;
  selectorGroup: string;
  nodeName: string;
}

export interface ClashBindingError {
  message: string;
  detail: string | null;
}

export interface ClashBindingController {
  /** Control URL input value (may be empty; the reader auto-detects). */
  controllerUrl: string;
  secret: string;
  snapshot: MihomoSnapshot | null;
  busy: boolean;
  /** One-line status after a successful probe or read. */
  status: string | null;
  error: ClashBindingError | null;
  selectedGroup: string;
  selectedNode: string;
  /** The binding to persist, or null when no group/node pair is active. */
  binding: ClashBindingSelection | null;
  setControllerUrl: (value: string) => void;
  setSecret: (value: string) => void;
  /** Connect and read the proxy groups; auto-selects the current node. */
  inspect: () => Promise<void>;
  /** Find the local Clash mixed port and control URL (probe only). */
  probe: () => Promise<LocalClashProbe | null>;
  selectGroup: (groupName: string) => void;
  selectNode: (nodeName: string) => void;
  /** Clear snapshot, selection, status, and error (draft reset paths). */
  reset: () => void;
  /**
   * Drop the read groups and selection while keeping the control URL and
   * secret inputs (network-mode switches, proxy imports, successful creates).
   */
  clearSnapshot: () => void;
  /** Ignore any in-flight response (Vault lock, surface teardown). */
  invalidate: () => void;
}

/** Picks the node a group currently points at, falling back to its first node. */
export function selectedNodeForGroup(
  group: MihomoSelectorGroup,
): string | null {
  const node =
    group.nodes.find((item) => item.name === group.selected) ?? group.nodes[0];
  return node?.name ?? null;
}

/** Converts a thrown error into the card-renderable message/detail pair. */
function toBindingError(error: unknown): ClashBindingError {
  const notice = errorNotice(error);
  return { message: notice.message, detail: notice.detail ?? null };
}

export function useClashBinding(options?: {
  initialControllerUrl?: string;
  /**
   * A binding carried over from a template, active until the groups are read
   * again (a copied Clash secret is never retained, so a re-read is expected).
   */
  initialSelection?: { selectorGroup: string; nodeName: string } | null;
  /** Called whenever the binding settles, so owners can persist it. */
  onBindingChange?: (binding: ClashBindingSelection) => void;
  /** Called only after a fresh read of the groups (not on manual re-selects). */
  onInspected?: (info: { controllerUrl: string; nodeName: string }) => void;
}): ClashBindingController {
  const [controllerUrl, setControllerUrlState] = useState(
    options?.initialControllerUrl ?? "",
  );
  const [secret, setSecret] = useState("");
  const [snapshot, setSnapshot] = useState<MihomoSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<ClashBindingError | null>(null);
  const [selectedGroup, setSelectedGroup] = useState(
    options?.initialSelection?.selectorGroup ?? "",
  );
  const [selectedNode, setSelectedNode] = useState(
    options?.initialSelection?.nodeName ?? "",
  );
  const requestRef = useRef(0);
  const epochRef = useRef(0);
  // Mount-time seed values; reset() restores them instead of chasing props.
  const seedRef = useRef({
    controllerUrl: options?.initialControllerUrl ?? "",
    selectorGroup: options?.initialSelection?.selectorGroup ?? "",
    nodeName: options?.initialSelection?.nodeName ?? "",
  });
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    return () => {
      // Drop any in-flight response once the owning surface unmounts.
      epochRef.current += 1;
    };
  }, []);

  const setControllerUrl = useCallback((value: string) => {
    setControllerUrlState(value);
    // A different control URL invalidates the previously read groups.
    setSnapshot(null);
    setSelectedGroup("");
    setSelectedNode("");
    setStatus(null);
    setError(null);
  }, []);

  const reset = useCallback(() => {
    requestRef.current += 1;
    setControllerUrlState(seedRef.current.controllerUrl);
    setSecret("");
    setSnapshot(null);
    setBusy(false);
    setStatus(null);
    setError(null);
    setSelectedGroup(seedRef.current.selectorGroup);
    setSelectedNode(seedRef.current.nodeName);
  }, []);

  const clearSnapshot = useCallback(() => {
    requestRef.current += 1;
    setSnapshot(null);
    setSelectedGroup("");
    setSelectedNode("");
    setStatus(null);
    setError(null);
  }, []);

  const invalidate = useCallback(() => {
    requestRef.current += 1;
    epochRef.current += 1;
    setBusy(false);
  }, []);

  const probe = useCallback(async (): Promise<LocalClashProbe | null> => {
    const requestId = ++requestRef.current;
    const epoch = epochRef.current;
    const isCurrent = () =>
      requestId === requestRef.current && epoch === epochRef.current;
    setError(null);
    setBusy(true);
    try {
      const probeResult = await desktopApi.probeLocalClash(secret);
      if (!isCurrent()) {
        return null;
      }
      if (probeResult.controllerUrl !== null) {
        setControllerUrlState(probeResult.controllerUrl);
      }
      setStatus(probeResult.detail);
      return probeResult;
    } catch (probeError) {
      if (!isCurrent()) {
        return null;
      }
      setStatus(null);
      setError(toBindingError(probeError));
      return null;
    } finally {
      if (isCurrent()) {
        setBusy(false);
      }
    }
  }, [secret]);

  const inspect = useCallback(async (): Promise<void> => {
    const requestId = ++requestRef.current;
    const epoch = epochRef.current;
    const isCurrent = () =>
      requestId === requestRef.current && epoch === epochRef.current;
    setError(null);
    setBusy(true);
    try {
      const inspected = await readMihomoGroups(controllerUrl, secret);
      if (!isCurrent()) {
        return;
      }
      setControllerUrlState(inspected.controllerUrl);
      const group = inspected.snapshot.groups[0];
      if (group === undefined || group.nodes.length === 0) {
        throw new UserFacingError("本机代理应用没有返回可用的线路分组。");
      }
      const node = selectedNodeForGroup(group);
      if (node === null) {
        throw new UserFacingError("所选线路分组中没有可用线路。");
      }
      setSnapshot(inspected.snapshot);
      setSelectedGroup(group.name);
      setSelectedNode(node);
      setStatus(
        `已读取代理组（${clashControllerLabel(inspected.controllerUrl)}）。`,
      );
      optionsRef.current?.onBindingChange?.({
        controllerUrl: inspected.controllerUrl,
        selectorGroup: group.name,
        nodeName: node,
      });
      optionsRef.current?.onInspected?.({
        controllerUrl: inspected.controllerUrl,
        nodeName: node,
      });
    } catch (inspectError) {
      if (!isCurrent()) {
        return;
      }
      setSnapshot(null);
      setSelectedGroup("");
      setSelectedNode("");
      setStatus(null);
      setError(toBindingError(inspectError));
    } finally {
      if (isCurrent()) {
        setBusy(false);
      }
    }
  }, [controllerUrl, secret]);

  const selectGroup = useCallback(
    (groupName: string) => {
      setSelectedGroup(groupName);
      const group = snapshot?.groups.find((item) => item.name === groupName);
      if (group === undefined) {
        setSelectedNode("");
        return;
      }
      const node = selectedNodeForGroup(group);
      setSelectedNode(node ?? "");
      if (node !== null && controllerUrl !== "") {
        optionsRef.current?.onBindingChange?.({
          controllerUrl,
          selectorGroup: group.name,
          nodeName: node,
        });
      }
    },
    [controllerUrl, snapshot],
  );

  const selectNode = useCallback(
    (nodeName: string) => {
      setSelectedNode(nodeName);
      if (selectedGroup !== "" && controllerUrl !== "" && nodeName !== "") {
        optionsRef.current?.onBindingChange?.({
          controllerUrl,
          selectorGroup: selectedGroup,
          nodeName,
        });
      }
    },
    [controllerUrl, selectedGroup],
  );

  const binding: ClashBindingSelection | null =
    controllerUrl !== "" &&
    (snapshot !== null || seedRef.current.nodeName !== "") &&
    selectedGroup !== "" &&
    selectedNode !== ""
      ? { controllerUrl, selectorGroup: selectedGroup, nodeName: selectedNode }
      : null;

  return {
    controllerUrl,
    secret,
    snapshot,
    busy,
    status,
    error,
    selectedGroup,
    selectedNode,
    binding,
    setControllerUrl,
    setSecret,
    inspect,
    probe,
    selectGroup,
    selectNode,
    reset,
    clearSnapshot,
    invalidate,
  };
}
