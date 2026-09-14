// Used only by the website's demo build. The installed desktop app does not
// import this module; its language and stored identity values stay unchanged.
export function demoText(zh: string, en: string): string {
  return document.documentElement.lang === "en" ? en : zh;
}

export function demoFormat(
  zh: string,
  en: string,
  values: readonly unknown[],
): string {
  return demoText(zh, en).replace(/\{(\d+)\}/g, (_, index: string) =>
    String(values[Number(index)]),
  );
}
