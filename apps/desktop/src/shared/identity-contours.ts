/** A visual recognition pattern from a public Silo ID; never an evidence hash. */
export function identityContours(id: string): string[] {
  let seed = 0;
  for (const char of id)
    seed = (Math.imul(seed, 31) + char.charCodeAt(0)) >>> 0;
  const phase = ((seed % 360) * Math.PI) / 180;
  const lobes = 3 + (seed % 3);
  return Array.from({ length: 15 }, (_, layer) => {
    const radius = 94 - layer * 5.1;
    const points = Array.from({ length: 80 }, (_, point) => {
      const angle = (point / 80) * Math.PI * 2;
      const wave =
        Math.sin(angle * lobes + phase + layer * 0.07) * (12 - layer * 0.55);
      const r = radius + wave;
      return `${(120 + Math.cos(angle) * r + layer * 0.7).toFixed(2)},${(120 + Math.sin(angle) * r * 0.88 - layer * 0.35).toFixed(2)}`;
    });
    return `M${points.join("L")}Z`;
  });
}
