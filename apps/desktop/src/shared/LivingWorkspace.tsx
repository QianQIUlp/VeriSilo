import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

export function useWorkspaceMotion() {
  const [preference, setPreference] = useState<"system" | "full" | "reduce">(
    () => {
      try {
        const saved = window.localStorage.getItem("verisilo.motion");
        if (saved === "full" || saved === "reduce") return saved;
      } catch {
        /* Preferences remain usable when local storage is unavailable. */
      }
      return "system";
    },
  );
  const [systemReduced, setSystemReduced] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  useEffect(() => {
    const syncPreference = (event: StorageEvent) => {
      if (event.key !== "verisilo.motion") return;
      setPreference(
        event.newValue === "full" || event.newValue === "reduce"
          ? event.newValue
          : "system",
      );
    };
    window.addEventListener("storage", syncPreference);
    return () => window.removeEventListener("storage", syncPreference);
  }, []);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setSystemReduced(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useLayoutEffect(() => {
    document.documentElement.dataset.motion = preference;
    try {
      window.localStorage.setItem("verisilo.motion", preference);
    } catch {
      /* Local preference only. */
    }
    return () => {
      delete document.documentElement.dataset.motion;
    };
  }, [preference]);
  return {
    preference,
    setPreference,
    reduced:
      preference === "reduce" || (preference === "system" && systemReduced),
    systemReduced,
  };
}

/** Animate the destination DOM directly; never snapshot Vault content for transitions. */
export function RouteStage({
  screen,
  reduced,
  children,
}: {
  screen: string;
  reduced: boolean;
  children: ReactNode;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const previous = useRef(screen);
  useLayoutEffect(() => {
    const node = surface.current;
    if (!node || previous.current === screen) return;
    const destinations = [
      "locked",
      "overview",
      "edit",
      "create",
      "settings",
      "environments",
      "cli",
      "tools",
    ];
    const direction =
      destinations.indexOf(screen) > destinations.indexOf(previous.current)
        ? 1
        : -1;
    const openingVault = previous.current === "locked" && screen === "overview";
    previous.current = screen;
    node.scrollTop = 0;
    if (reduced) return;
    const animation = node.animate(
      [
        {
          opacity: 0,
          transform: openingVault
            ? "translateY(0) scale(.84)"
            : `translate(${direction * 55}px, 18px) scale(.96)`,
          filter: "blur(5px)",
        },
        { opacity: 1, transform: "translateY(0) scale(1)", filter: "blur(0)" },
      ],
      { duration: 620, easing: "cubic-bezier(.16,1,.3,1)" },
    );
    return () => {
      animation.cancel();
    };
  }, [screen, reduced]);
  return (
    <div ref={surface} className="route-stage" data-screen={screen}>
      {children}
    </div>
  );
}

export function WorldBackdrop({ screen }: { screen: string }) {
  return (
    <div className="world-backdrop" data-world={screen} aria-hidden="true">
      <div className="world-light" />
      <div className="world-orbits">
        <i />
        <i />
        <i />
      </div>
      <span className="world-coordinate">VERISILO / LOCAL SPACE</span>
    </div>
  );
}

export function InstrumentObject({
  kind,
  active = false,
}: {
  kind: "vault" | "browser" | "linux" | "remote" | "command" | "tools";
  active?: boolean;
}) {
  return (
    <div
      className={`instrument-object object-${kind}${active ? " object-active" : ""}`}
      aria-hidden="true"
    >
      <div className="object-halo" />
      <div className="object-body">
        <div className="object-layer layer-back" />
        <div className="object-layer layer-middle" />
        <div className="object-layer layer-front">
          <svg
            viewBox="0 0 100 100"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.2"
          >
            {kind === "vault" ? (
              <>
                <circle cx="50" cy="50" r="27" />
                <circle cx="50" cy="50" r="15" />
                <path d="M50 14v16m0 40v16M14 50h16m40 0h16M25 25l12 12m26 26 12 12M25 75l12-12m26-26 12-12" />
              </>
            ) : kind === "command" ? (
              <>
                <path d="m23 31 21 19-21 19M52 68h26" />
                <path d="M20 15h60M20 85h60" opacity=".3" />
              </>
            ) : kind === "linux" ? (
              <>
                <path d="M24 31 50 17l26 14v38L50 83 24 69ZM24 31l26 16 26-16M50 47v36" />
                <path d="m37 40 26-16M37 40v22l26 15" opacity=".4" />
              </>
            ) : kind === "remote" ? (
              <>
                <circle cx="50" cy="50" r="29" />
                <ellipse cx="50" cy="50" rx="13" ry="29" />
                <path d="M21 50h58M26 35h48M26 65h48" />
              </>
            ) : kind === "tools" ? (
              <>
                <path d="M22 30h56M22 50h56M22 70h56M35 21v18M66 41v18M44 61v18" />
              </>
            ) : (
              <>
                <rect x="19" y="24" width="62" height="47" rx="5" />
                <path d="M19 36h62M38 82h24M50 71v11" />
                <circle cx="27" cy="30" r="1" />
              </>
            )}
          </svg>
        </div>
      </div>
      <span className="object-ground" />
    </div>
  );
}

export function ToolStudio({
  archive,
  network,
  reports,
  status,
}: {
  archive: ReactNode;
  network: ReactNode;
  reports: ReactNode;
  status: ReactNode;
}) {
  const [selected, setSelected] = useState<
    "status" | "archive" | "network" | "reports"
  >("status");
  const items = [
    { id: "status", name: "本机状态", code: "01", copy: "看见当前工作环境" },
    { id: "archive", name: "归档空间", code: "02", copy: "找回暂时收起的身份" },
    { id: "network", name: "网络检查", code: "03", copy: "查看出口与检查记录" },
    {
      id: "reports",
      name: "证据报告",
      code: "04",
      copy: "将已有证据带出工作区",
    },
  ] as const;
  return (
    <section className="tool-studio living-page">
      <header className="living-heading">
        <p className="eyebrow">06 / WORKBENCH</p>
        <h1>工具，在手边。</h1>
        <p>选择一件工具，把注意力留给眼前的事。</p>
      </header>
      <div className="workbench-layout">
        <nav className="tool-selector" aria-label="本机工具类别">
          {items.map((item) => (
            <button
              type="button"
              key={item.id}
              aria-pressed={selected === item.id}
              onClick={() => setSelected(item.id)}
            >
              <span>{item.code}</span>
              <strong>{item.name}</strong>
              <small>{item.copy}</small>
              <i aria-hidden="true">↗</i>
            </button>
          ))}
        </nav>
        <div className="workbench-surface" key={selected}>
          <div className="workbench-index" aria-hidden="true">
            {items.find((item) => item.id === selected)?.code}
          </div>
          {selected === "status"
            ? status
            : selected === "archive"
              ? archive
              : selected === "network"
                ? network
                : reports}
        </div>
      </div>
    </section>
  );
}
