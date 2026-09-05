import type {
  BrowserKind,
  NetworkProfile,
  SiloExecutionTarget,
} from "@verisilo/contracts";
import { useCallback, useRef, useState } from "react";
import type { MihomoSnapshot, WslStatus } from "../../desktop-api.js";
import {
  defaultColor,
  defaultMihomoControllerUrl,
  emptyNetwork,
  type WslCreationOption,
} from "../../shared/defaults.js";

// The coordinator resets this entire draft when a Vault session becomes invalid.
// Refs remain stable so in-flight discovery/controller results can be discarded.
export function useSiloDraft() {
  const [name, setName] = useState("");
  const [color, setColor] = useState(defaultColor);
  const [browserPath, setBrowserPath] = useState("");
  const [browserKind, setBrowserKind] = useState<BrowserKind>("chrome");
  const [executionTarget, setExecutionTarget] = useState<SiloExecutionTarget>({
    kind: "local",
  });
  const [createWslStatus, setCreateWslStatus] = useState<WslStatus | null>(
    null,
  );
  const [createWslOptions, setCreateWslOptions] = useState<WslCreationOption[]>(
    [],
  );
  const [createWslBusy, setCreateWslBusy] = useState(false);
  const [networkProfile, setNetworkProfile] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [mihomoControllerUrl, setMihomoControllerUrl] = useState(
    defaultMihomoControllerUrl,
  );
  const [mihomoControllerSecret, setMihomoControllerSecret] = useState("");
  const [mihomoSnapshot, setMihomoSnapshot] = useState<MihomoSnapshot | null>(
    null,
  );
  const [mihomoBusy, setMihomoBusy] = useState(false);
  const mihomoRequestRef = useRef(0);
  const createWslRequestRef = useRef(0);
  const browserSelectionExplicitRef = useRef(false);

  const resetSiloDraft = useCallback(() => {
    setName("");
    setColor(defaultColor);
    setBrowserPath("");
    setBrowserKind("chrome");
    browserSelectionExplicitRef.current = false;
    setExecutionTarget({ kind: "local" });
    setCreateWslStatus(null);
    setCreateWslOptions([]);
    setCreateWslBusy(false);
    createWslRequestRef.current += 1;
    setNetworkProfile(emptyNetwork());
    setProxyImport("");
    setProxyUsername("");
    setProxyPassword("");
    setMihomoControllerUrl(defaultMihomoControllerUrl);
    setMihomoControllerSecret("");
    setMihomoSnapshot(null);
    setMihomoBusy(false);
  }, []);
  return {
    name,
    setName,
    color,
    setColor,
    browserPath,
    setBrowserPath,
    browserKind,
    setBrowserKind,
    executionTarget,
    setExecutionTarget,
    createWslStatus,
    setCreateWslStatus,
    createWslOptions,
    setCreateWslOptions,
    createWslBusy,
    setCreateWslBusy,
    networkProfile,
    setNetworkProfile,
    proxyImport,
    setProxyImport,
    proxyUsername,
    setProxyUsername,
    proxyPassword,
    setProxyPassword,
    mihomoControllerUrl,
    setMihomoControllerUrl,
    mihomoControllerSecret,
    setMihomoControllerSecret,
    mihomoSnapshot,
    setMihomoSnapshot,
    mihomoBusy,
    setMihomoBusy,
    mihomoRequestRef,
    createWslRequestRef,
    browserSelectionExplicitRef,
    resetSiloDraft,
  };
}
