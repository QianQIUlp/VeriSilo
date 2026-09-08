// @ts-expect-error -- tests run in Node, while the production tsconfig omits Node globals.
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ManagedSiloForm } from "./ManagedSiloForm.js";

const formSource = readFileSync(
  new URL("./ManagedSiloForm.tsx", import.meta.url),
  "utf8",
);

const renderForm = (name: string) =>
  renderToStaticMarkup(
    createElement(ManagedSiloForm, {
      busy: false,
      initialColor: "#5b5ce2",
      name,
      onNameChange: () => {},
      color: "#5b5ce2",
      onColorChange: () => {},
      onSubmit: async () => {},
    }),
  );

describe("ManagedSiloForm creation feedback", () => {
  it("renders shared identity fields and hides the blocker hint when named", () => {
    const markup = renderForm("保留名称");
    expect(markup).toContain('value="保留名称"');
    expect(markup).not.toContain("创建前还需要");
  });

  it("names the missing prerequisite instead of silently disabling submit", () => {
    const markup = renderForm("");
    expect(markup).toContain("创建前还需要：填写 Silo 名称。");
  });
});

describe("ManagedSiloForm progressive disclosure", () => {
  it("keeps the default path to user-level choices only", () => {
    const markup = renderForm("保留名称");
    expect(markup).toContain('id="managed-silo-name"');
    expect(markup).toContain('id="managed-identity-preset"');
    expect(markup).toContain("Direct 直连");
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).toContain("高级身份设置");
    expect(markup).toContain("使用安全默认值");
    expect(markup).not.toContain('id="managed-timezone"');
    expect(markup).not.toContain('id="managed-screen"');
    expect(markup).not.toContain('id="managed-cores"');
    expect(markup).not.toContain('id="managed-gpu"');
    expect(markup).not.toContain("User-Agent");
    expect(markup).not.toContain("Canvas / Audio");
    expect(markup).not.toContain("GPU / WebGL");
  });

  it("keeps real advanced knobs reachable behind the collapsed toggle", () => {
    const toggleIndex = formSource.indexOf('className="create-advanced-toggle"');
    const advancedIndex = formSource.indexOf("{advancedOpen ? (", toggleIndex);
    expect(toggleIndex).toBeGreaterThan(-1);
    expect(advancedIndex).toBeGreaterThan(toggleIndex);
    for (const id of [
      "managed-timezone",
      "managed-screen",
      "managed-cores",
      "managed-gpu",
    ]) {
      const knobIndex = formSource.indexOf(`id="${id}"`);
      expect(knobIndex).toBeGreaterThan(advancedIndex);
    }
    expect(formSource).toContain("User-Agent 跟随内置 Firefox 内核");
  });

  it("keeps default submit semantics unchanged by the collapsed advanced section", () => {
    expect(formSource).toContain("identityPreset,");
    expect(formSource).toContain("screenWidth,");
    expect(formSource).toContain("screenHeight,");
    expect(formSource).toContain("hardwareConcurrency:");
    expect(formSource).toContain("gpuPreset:");
    expect(formSource).toContain("timezone: hasProxy && followNetworkExit");
    expect(formSource).toContain('useState<ManagedIdentityPreset>("balanced-zh-cn")');
    expect(formSource).toContain('useState("auto")');
    expect(formSource).toContain('useState<number | "">');
  });
});
