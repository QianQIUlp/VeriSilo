export const GPU_PRESETS = [
  {
    id: "auto",
    label: "由引擎生成（推荐）",
    vendor: null,
    renderer: null,
  },
  {
    id: "nvidia-rtx-3060",
    label: "NVIDIA GeForce RTX 3060",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce RTX 3060, or similar",
  },
  {
    id: "nvidia-rtx-3070",
    label: "NVIDIA GeForce RTX 3070",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce RTX 3070, or similar",
  },
  {
    id: "nvidia-rtx-4060",
    label: "NVIDIA GeForce RTX 4060",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce RTX 4060, or similar",
  },
  {
    id: "nvidia-rtx-4070",
    label: "NVIDIA GeForce RTX 4070",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce RTX 4070, or similar",
  },
  {
    id: "nvidia-rtx-4080",
    label: "NVIDIA GeForce RTX 4080",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce RTX 4080, or similar",
  },
  {
    id: "nvidia-gtx-1660",
    label: "NVIDIA GeForce GTX 1660",
    vendor: "NVIDIA Corporation",
    renderer: "NVIDIA GeForce GTX 1660, or similar",
  },
  {
    id: "amd-rx-6600",
    label: "AMD Radeon RX 6600",
    vendor: "ATI Technologies Inc.",
    renderer: "AMD Radeon RX 6600, or similar",
  },
  {
    id: "amd-rx-7600",
    label: "AMD Radeon RX 7600",
    vendor: "ATI Technologies Inc.",
    renderer: "AMD Radeon RX 7600, or similar",
  },
  {
    id: "amd-rx-7800xt",
    label: "AMD Radeon RX 7800 XT",
    vendor: "ATI Technologies Inc.",
    renderer: "AMD Radeon RX 7800 XT, or similar",
  },
  {
    id: "intel-uhd-770",
    label: "Intel UHD Graphics 770",
    vendor: "Intel",
    renderer: "Intel(R) UHD Graphics 770, or similar",
  },
] as const;

export type GpuPresetId = (typeof GPU_PRESETS)[number]["id"];

export function gpuPresetFromWebgl(
  vendor: string,
  renderer: string,
): GpuPresetId {
  const match = GPU_PRESETS.find(
    (preset) => preset.vendor === vendor && preset.renderer === renderer,
  );
  return match?.id ?? "auto";
}
