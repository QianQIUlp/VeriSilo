// @ts-expect-error -- tests run in Node, while the production tsconfig omits Node globals.
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ManagedSiloForm } from "./ManagedSiloForm.js";
import type { ManagedSiloTemplate } from "./managedTemplate.js";

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

const remoteProxyTemplate: ManagedSiloTemplate = {
  sourceSiloId: "11111111-1111-4111-8111-111111111111",
  sourceSiloName: "工作空间",
  name: "工作空间 - 新身份",
  color: "#128f8b",
  identityPreset: "balanced-en-us",
  followNetworkExit: true,
  screenWidth: 1920,
  screenHeight: 1080,
  hardwareConcurrency: 8,
  gpuPreset: "nvidia-rtx-4060",
  timezone: "America/New_York",
  networkMode: "remote",
  proxyScheme: "http",
  proxyHost: "proxy.example.test",
  proxyPort: "3128",
  mixedPort: "7897",
  controllerUrl: "",
  selectorGroup: "",
  nodeName: "",
  proxyCredentialUsed: true,
  mihomoSecretUsed: false,
};

const renderTemplatedForm = (template: ManagedSiloTemplate) =>
  renderToStaticMarkup(
    createElement(ManagedSiloForm, {
      busy: false,
      initialColor: "#5b5ce2",
      name: template.name,
      onNameChange: () => {},
      color: template.color,
      onColorChange: () => {},
      onSubmit: async () => {},
      template,
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
    expect(formSource).toContain(
      'useState<ManagedIdentityPreset>(seed?.identityPreset ?? "balanced-zh-cn")',
    );
    expect(formSource).toContain('useState<string>(seed?.gpuPreset ?? "auto")');
    expect(formSource).toContain('useState<number | "">');
  });
});

describe("ManagedSiloForm create-new-identity template", () => {
  it("frames the form as creating a new identity from the source Silo", () => {
    const markup = renderTemplatedForm(remoteProxyTemplate);
    expect(markup).toContain("创建新身份");
    expect(markup).toContain("以此配置创建新的托管身份浏览器");
    expect(markup).toContain("原 Silo 不会被修改");
  });

  it("prefills network, identity, and advanced values from the template", () => {
    const markup = renderTemplatedForm(remoteProxyTemplate);
    expect(markup).toContain('id="managed-proxy-host"');
    expect(markup).toContain('value="proxy.example.test"');
    expect(markup).toContain('value="3128"');
    expect(markup).toContain('value="balanced-en-us"');
    expect(markup).toContain('value="America/New_York"');
    expect(markup).toContain('value="nvidia-rtx-4060"');
    expect(markup).toContain('value="8"');
    // The advanced section opens so the inherited values are reviewable.
    expect(markup).toContain('aria-expanded="true"');
    expect(markup).toContain('id="managed-gpu"');
  });

  it("never prefills proxy credential fields", () => {
    const markup = renderTemplatedForm(remoteProxyTemplate);
    expect(markup).toContain('id="managed-proxy-username"');
    expect(markup).toContain('id="managed-proxy-password"');
    expect(markup).not.toContain('id="managed-proxy-username"\n                value=');
    expect(markup).toContain("不会自动复制");
  });

  it("shows the Clash secret re-entry hint for bound templates", () => {
    const markup = renderTemplatedForm({
      ...remoteProxyTemplate,
      networkMode: "clash",
      proxyScheme: "socks5",
      proxyHost: "127.0.0.1",
      proxyPort: "7897",
      mixedPort: "7897",
      controllerUrl: "http://127.0.0.1:9097",
      selectorGroup: "全局",
      nodeName: "香港 01",
      proxyCredentialUsed: false,
      mihomoSecretUsed: true,
    });
    expect(markup).toContain('value="7897"');
    // The controller URL is prefilled; the field displays its port portion.
    expect(markup).toContain('value="9097"');
    expect(markup).toContain("原 Clash 密钥不会自动复制");
  });

  it("keeps an inherited screen size selectable even when outside the standard list", () => {
    const markup = renderTemplatedForm({
      ...remoteProxyTemplate,
      screenWidth: 1680,
      screenHeight: 1050,
    });
    expect(markup).toContain('value="1680x1050"');
  });
});
