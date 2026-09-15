// The landing page and the embedded app share only a motion preference.
const root = document.documentElement;
const motion = document.querySelector<HTMLSelectElement>("#site-motion")!;
const systemMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
let preference = "system";
let reduced = systemMotion.matches;
try {
  preference = localStorage.getItem("verisilo.motion") ?? "system";
} catch {
  /* The experience also works without browser storage. */
}

const universe = document.querySelector<HTMLElement>(".identity-universe")!;
const identity = document.querySelector<HTMLButtonElement>(".hero-identity")!;
const portal = document.querySelector<HTMLElement>("#portal-window")!;
const berth = document.querySelector<HTMLElement>(".portal-berth")!;
const frame = document.querySelector<HTMLIFrameElement>("#demo-frame")!;
const expand = document.querySelector<HTMLButtonElement>("#demo-expand")!;
const canvas = document.querySelector<HTMLElement>(".portal-canvas")!;
new ResizeObserver(() => {
  canvas.style.setProperty(
    "--demo-scale",
    String(Math.min(1, canvas.clientWidth / 900)),
  );
}).observe(canvas);
let scrollFrame = 0;
const clamp = (value: number) => Math.min(1, Math.max(0, value));
function updateLandscape() {
  scrollFrame = 0;
  const progress = clamp(
    1 - berth.getBoundingClientRect().top / window.innerHeight,
  );
  root.style.setProperty(
    "--journey",
    String(reduced ? 0 : clamp(window.scrollY / window.innerHeight)),
  );
  root.style.setProperty("--portal-progress", String(reduced ? 1 : progress));
}
function scheduleLandscape() {
  if (!scrollFrame) scrollFrame = requestAnimationFrame(updateLandscape);
}
function applyMotion(value: string) {
  preference = value === "full" || value === "reduce" ? value : "system";
  root.dataset.motion = preference;
  motion.value = preference;
  reduced =
    preference === "reduce" ||
    (preference === "system" && systemMotion.matches);
  identity.style.removeProperty("--pointer-x");
  identity.style.removeProperty("--pointer-y");
  scheduleLandscape();
}
applyMotion(preference);
motion.addEventListener("change", () => {
  applyMotion(motion.value);
  try {
    localStorage.setItem("verisilo.motion", preference);
  } catch {
    /* Optional preference. */
  }
});
window.addEventListener("storage", (event) => {
  if (event.key === "verisilo.motion") applyMotion(event.newValue ?? "system");
});
systemMotion.addEventListener("change", () => applyMotion(preference));
window.addEventListener("scroll", scheduleLandscape, { passive: true });
window.addEventListener("resize", scheduleLandscape);

function selectIdentity(index: number) {
  universe.dataset.identity = String(index);
  document
    .querySelectorAll<HTMLElement>(".hero-contour, .identity-caption > div")
    .forEach((node, i) => {
      node.hidden = i % 3 !== index;
    });
  document
    .querySelectorAll<HTMLButtonElement>("[data-identity-choice]")
    .forEach((button, i) => {
      button.setAttribute("aria-pressed", String(i === index));
    });
}
identity.addEventListener("click", () =>
  selectIdentity((Number(universe.dataset.identity) + 1) % 3),
);
document
  .querySelectorAll<HTMLButtonElement>("[data-identity-choice]")
  .forEach((button, index) => {
    button.addEventListener("click", () => selectIdentity(index));
  });
identity.addEventListener("pointermove", (event) => {
  if (reduced || event.pointerType === "touch") return;
  const rect = identity.getBoundingClientRect();
  identity.style.setProperty(
    "--pointer-x",
    `${(event.clientX - rect.left - rect.width / 2) / 24}px`,
  );
  identity.style.setProperty(
    "--pointer-y",
    `${(event.clientY - rect.top - rect.height / 2) / 24}px`,
  );
});
identity.addEventListener("pointerleave", () => {
  identity.style.removeProperty("--pointer-x");
  identity.style.removeProperty("--pointer-y");
});

const lab = document.querySelector<HTMLElement>(".evidence-lab")!;
document
  .querySelectorAll<HTMLButtonElement>("[data-evidence-choice]")
  .forEach((button, index, buttons) => {
    button.addEventListener("click", () => {
      lab.dataset.evidence = String(index);
      buttons.forEach((item, i) =>
        item.setAttribute("aria-pressed", String(i === index)),
      );
      lab
        .querySelectorAll<HTMLElement>(
          ".observation-result > strong, .lab-result > div",
        )
        .forEach((node, i) => {
          node.hidden = i % 3 !== index;
        });
      lab.querySelector(".evidence-connector > span")!.textContent = [
        "≍",
        "≠",
        "∅",
      ][index]!;
    });
  });

let expanded = false;
let previousOverflow = "";
const inertSiblings = new Map<HTMLElement, boolean>();
let portalAnimation: Animation | undefined;
function setExpanded(value: boolean) {
  if (value === expanded) return;
  portalAnimation?.cancel();
  const before = portal.getBoundingClientRect();
  // Preserve the page's geometry while the window is fixed to the viewport.
  if (value) berth.style.height = `${portal.offsetHeight}px`;
  expanded = value;
  portal.classList.toggle("is-expanded", value);
  document.body.classList.toggle("demo-expanded", value);
  expand.setAttribute("aria-expanded", String(value));
  expand.textContent =
    (value ? expand.dataset.closeLabel : expand.dataset.openLabel) ?? "";
  if (value) {
    previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    portal.setAttribute("role", "dialog");
    portal.setAttribute("aria-modal", "true");
    portal.setAttribute("aria-label", frame.title);
    // Keep the iframe in place: moving it would reset the visitor's demo session.
    let ancestor: HTMLElement = portal;
    while (ancestor.parentElement) {
      for (const sibling of ancestor.parentElement.children) {
        if (sibling !== ancestor && sibling instanceof HTMLElement) {
          inertSiblings.set(sibling, sibling.inert);
          sibling.inert = true;
        }
      }
      ancestor = ancestor.parentElement;
      if (ancestor === document.body) break;
    }
  } else {
    document.body.style.overflow = previousOverflow;
    berth.style.removeProperty("height");
    portal.removeAttribute("role");
    portal.removeAttribute("aria-modal");
    portal.removeAttribute("aria-label");
    inertSiblings.forEach((wasInert, node) => {
      node.inert = wasInert;
    });
    inertSiblings.clear();
    scheduleLandscape();
  }
  if (!reduced && value) {
    const after = portal.getBoundingClientRect();
    portalAnimation = portal.animate(
      [
        {
          transformOrigin: "0 0",
          transform: `translate(${before.left - after.left}px, ${before.top - after.top}px) scale(${before.width / after.width}, ${before.height / after.height})`,
        },
        { transformOrigin: "0 0", transform: "none" },
      ],
      { duration: 620, easing: "cubic-bezier(.16,1,.3,1)" },
    );
  }
  expand.focus({ preventScroll: true });
}
expand.addEventListener("click", () => setExpanded(!expanded));
window.addEventListener("keydown", (event) => {
  if (expanded && event.key === "Tab") {
    const last = portal.querySelector<HTMLAnchorElement>(".portal-caption a")!;
    if (event.shiftKey && document.activeElement === expand) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      expand.focus();
    }
  }
  if (event.key === "Escape" && expanded) {
    event.preventDefault();
    setExpanded(false);
  }
});
window.addEventListener("message", (event) => {
  if (event.origin !== location.origin || event.source !== frame.contentWindow)
    return;
  if (event.data?.type === "verisilo-demo-ready")
    portal.classList.add("is-ready");
  if (event.data?.type === "verisilo-demo-exit") setExpanded(false);
});
// Covers a cached iframe finishing before this module was evaluated.
if (frame.contentDocument?.querySelector(".spatial-shell"))
  portal.classList.add("is-ready");
