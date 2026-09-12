import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";
import { IdentityMark } from "./IdentityMark.js";

it("keeps recognition geometry stable across rerenders and color changes", () => {
  const render = (id: string, color: string) =>
    renderToStaticMarkup(createElement(IdentityMark, { id, color }));
  const shape = (markup: string) =>
    [...markup.matchAll(/ d="([^"]+)"/g)].map((match) => match[1]);
  const original = render("persistent-silo-a", "#176c68");
  expect(shape(original)).toEqual(
    shape(render("persistent-silo-a", "#a43f37")),
  );
  expect(shape(original)).not.toEqual(
    shape(render("persistent-silo-b", "#176c68")),
  );
  expect(original).toContain('aria-hidden="true"');
  expect(original).not.toMatch(/NaN|Infinity|verified/i);
});
