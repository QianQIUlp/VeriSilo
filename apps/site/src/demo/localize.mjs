import { fileURLToPath } from "node:url";
import ts from "typescript";
import catalog from "./en.json" with { type: "json" };

const desktopRoot = fileURLToPath(
  new URL("../../../desktop/src/", import.meta.url),
).replaceAll("\\", "/");
const helper = `${desktopRoot}preview/public-language.ts`;

// Localize authored copy at the website build boundary. Never scan rendered
// text: a visitor's Silo name, command, URL, or observed evidence is runtime data.
export function localizeDemo(code, id, messages = catalog) {
  const file = ts.createSourceFile(
    id,
    code,
    ts.ScriptTarget.Latest,
    true,
    id.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  let changed = false;
  const result = ts.transform(file, [
    (context) => {
      const f = context.factory;
      const translate = (zh, values) => {
        if (!Object.hasOwn(messages, zh)) {
          if (/[\u3400-\u9fff]/.test(zh))
            throw new Error(`Missing public demo translation in ${id}: ${zh}`);
          return undefined;
        }
        changed = true;
        return f.createCallExpression(
          f.createIdentifier(values ? "__demoFormat" : "__demoText"),
          undefined,
          [
            f.createStringLiteral(zh),
            f.createStringLiteral(messages[zh]),
            ...(values ? [f.createArrayLiteralExpression(values)] : []),
          ],
        );
      };
      const visit = (node) => {
        // Property/type names and comparisons are program semantics, not copy.
        // In particular, Chinese backend error prefixes must still match Chinese
        // messages. The public demo never receives real backend messages.
        if (
          ts.isTypeNode(node) ||
          ts.isImportDeclaration(node) ||
          ts.isExportDeclaration(node)
        )
          return node;
        if (ts.isStringLiteral(node)) {
          const p = node.parent;
          if (
            (p.name === node && !ts.isJsxAttribute(p)) ||
            ts.isElementAccessExpression(p) ||
            (ts.isBinaryExpression(p) &&
              [
                ts.SyntaxKind.EqualsEqualsToken,
                ts.SyntaxKind.EqualsEqualsEqualsToken,
                ts.SyntaxKind.ExclamationEqualsToken,
                ts.SyntaxKind.ExclamationEqualsEqualsToken,
              ].includes(p.operatorToken.kind)) ||
            (ts.isCallExpression(p) &&
              ts.isPropertyAccessExpression(p.expression) &&
              [
                "startsWith",
                "endsWith",
                "includes",
                "indexOf",
                "replace",
                "replaceAll",
              ].includes(p.expression.name.text))
          )
            return node;
          // Only date presentation uses the UI locale. Identity language values
          // such as zh-CN remain untouched everywhere else.
          const dateLocale =
            node.text === "zh-CN" &&
            ts.isNewExpression(p) &&
            p.expression.getText(file) === "Intl.DateTimeFormat";
          const replacement = dateLocale
            ? ((changed = true),
              f.createCallExpression(
                f.createIdentifier("__demoText"),
                undefined,
                [
                  f.createStringLiteral("zh-CN"),
                  f.createStringLiteral("en-US"),
                ],
              ))
            : translate(node.text);
          if (replacement)
            return ts.isJsxAttribute(p)
              ? f.createJsxExpression(undefined, replacement)
              : replacement;
        }
        if (ts.isNoSubstitutionTemplateLiteral(node))
          return translate(node.text) ?? node;
        if (ts.isTemplateExpression(node)) {
          const key =
            node.head.text +
            node.templateSpans
              .map((span, index) => `{${index}}${span.literal.text}`)
              .join("");
          const replacement = translate(
            key,
            node.templateSpans.map((span) =>
              ts.visitNode(span.expression, visit),
            ),
          );
          if (replacement) return replacement;
        }
        if (ts.isJsxText(node)) {
          const key = node.text
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter(Boolean)
            .join(" ");
          const replacement = translate(key);
          if (replacement) return f.createJsxExpression(undefined, replacement);
        }
        return ts.visitEachChild(node, visit, context);
      };
      return (root) => ts.visitNode(root, visit);
    },
  ]);
  if (!changed) {
    result.dispose();
    return null;
  }
  const output = ts
    .createPrinter({ newLine: ts.NewLineKind.LineFeed })
    .printFile(result.transformed[0]);
  result.dispose();
  return `import { demoText as __demoText, demoFormat as __demoFormat } from ${JSON.stringify(helper)};\n${output}`;
}

export function demoLocalization() {
  return {
    name: "verisilo-public-demo-language",
    enforce: "pre",
    transform(code, id) {
      const path = id.split("?")[0].replaceAll("\\", "/");
      if (
        !path.startsWith(desktopRoot) ||
        !/\.(ts|tsx)$/.test(path) ||
        /(?:\.test\.|desktop-api\.ts$|public-language\.ts$)/.test(path)
      )
        return null;
      const localized = localizeDemo(code, path);
      return localized === null ? null : { code: localized, map: null };
    },
  };
}
