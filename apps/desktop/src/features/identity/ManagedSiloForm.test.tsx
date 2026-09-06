import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ManagedSiloForm } from "./ManagedSiloForm.js";

describe("ManagedSiloForm creation feedback", () => {
  it("renders shared identity fields and hides the blocker hint when named", () => {
    const markup = renderToStaticMarkup(
      createElement(ManagedSiloForm, {
        busy: false,
        initialColor: "#5b5ce2",
        name: "保留名称",
        onNameChange: () => {},
        color: "#5b5ce2",
        onColorChange: () => {},
        onSubmit: async () => {},
      }),
    );
    expect(markup).toContain('value="保留名称"');
    expect(markup).not.toContain("创建前还需要");
  });

  it("names the missing prerequisite instead of silently disabling submit", () => {
    const markup = renderToStaticMarkup(
      createElement(ManagedSiloForm, {
        busy: false,
        initialColor: "#5b5ce2",
        name: "",
        onNameChange: () => {},
        color: "#5b5ce2",
        onColorChange: () => {},
        onSubmit: async () => {},
      }),
    );
    expect(markup).toContain("创建前还需要：填写 Silo 名称。");
  });
});
