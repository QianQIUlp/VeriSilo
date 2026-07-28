export function canConfigureWslDistribution(
  distributions: readonly string[],
  selected: string,
): boolean {
  return selected !== "" && distributions.includes(selected);
}

export function requiresExplicitWslSelection(
  distributions: readonly string[],
  selected: string,
): boolean {
  return (
    distributions.length > 1 &&
    !canConfigureWslDistribution(distributions, selected)
  );
}
