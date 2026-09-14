import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { demoLocalization, localizeDemo } from "./localize.mjs";

const runtime = ts.transpileModule(
  readFileSync(
    new URL("../../../desktop/src/preview/public-language.ts", import.meta.url),
    "utf8",
  ),
  {
    compilerOptions: { module: ts.ModuleKind.CommonJS },
  },
).outputText;

function run(source, lang, messages) {
  const exports = {};
  const context = vm.createContext({
    exports,
    document: { documentElement: { lang } },
  });
  vm.runInContext(runtime, context);
  context.__demoText = exports.demoText;
  context.__demoFormat = exports.demoFormat;
  context.React = {
    createElement: (tag, props, ...children) => ({ tag, props, children }),
  };
  const localized = localizeDemo(source, "example.tsx", messages) ?? source;
  const code = ts.transpileModule(
    localized.replace(/^import .*__demoText.*\n/, ""),
    {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        jsx: ts.JsxEmit.React,
      },
    },
  ).outputText;
  vm.runInContext(code, context);
  return exports;
}

test("localizes authored JSX, attributes, labels, and templates in English and Chinese", () => {
  const source = `export function view(name: string) {
    return <button aria-label="创建 Silo" title={\`已创建「\${name}」。\`}>创建 Silo</button>;
  }`;
  const messages = {
    "创建 Silo": "Create Silo",
    "已创建「{0}」。": "Created “{0}”.",
  };
  const en = run(source, "en", messages).view("我的身份");
  assert.equal(en.props["aria-label"], "Create Silo");
  assert.equal(en.props.title, "Created “我的身份”.");
  assert.equal(en.children[0], "Create Silo");
  const zh = run(source, "zh-CN", messages).view("我的身份");
  assert.equal(zh.props.title, "已创建「我的身份」。");
  assert.equal(zh.children[0], "创建 Silo");
});

test("interpolations are evaluated once and user data is never translated recursively", () => {
  const { message } = run(
    `export function message(value: string) {
    let calls = 0;
    const result = \`创建 \${(++calls, value)}：\${"名称"}\`;
    return { result, calls };
  }`,
    "en",
    { "创建 {0}：{1}": "Create {0}: {1}", 名称: "Name" },
  );
  assert.equal(message("名称 {1} $&").result, "Create 名称 {1} $&: Name");
  assert.equal(message("x").calls, 1);
});

test("preserves property keys, comparisons, backend error matching, and identity language values", () => {
  const { value, matches, locale } = run(
    `
    export const value = { "名称": "名称", language: "zh-CN" };
    export const matches = (raw: string) => raw.startsWith("名称") && raw !== "创建";
    export const locale = "zh-CN";
  `,
    "en",
    { 名称: "Name", 创建: "Create" },
  );
  assert.equal(value["名称"], "Name");
  assert.equal(value.language, "zh-CN");
  assert.equal(locale, "zh-CN");
  assert.equal(matches("名称错误"), true);
});

test("new authored Chinese copy fails the site build until translated", () => {
  assert.throws(
    () => localizeDemo('const label = "未翻译";', "test.ts", {}),
    /Missing public demo translation/,
  );
});

test("translation placeholders stay complete and English messages contain no untranslated Han text", () => {
  const catalog = JSON.parse(
    readFileSync(new URL("./en.json", import.meta.url), "utf8"),
  );
  const slots = (value) =>
    [...value.matchAll(/\{(\d+)\}/g)].map((match) => match[0]).sort();
  for (const [zh, en] of Object.entries(catalog)) {
    assert.deepEqual(slots(en), slots(zh), zh);
    assert.doesNotMatch(en, /[\u3400-\u9fff]/, zh);
  }
});

test("the build plugin filters contracts, bridge, helper, and test files", () => {
  const plugin = demoLocalization();
  assert.equal(
    plugin.transform(
      'const label = "名称";',
      "/packages/contracts/src/index.ts",
    ),
    null,
  );
  for (const path of [
    "desktop-api.ts",
    "preview/public-language.ts",
    "App.test.tsx",
  ]) {
    const excluded = fileURLToPath(
      new URL(`../../../desktop/src/${path}`, import.meta.url),
    );
    assert.equal(plugin.transform('const label = "名称";', excluded), null);
  }
  const source = fileURLToPath(
    new URL("../../../desktop/src/App.tsx", import.meta.url),
  );
  assert.match(
    plugin.transform('const label = "名称";', source).code,
    /__demoText/,
  );
});
