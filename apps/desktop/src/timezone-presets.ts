export const TIMEZONE_PRESETS = [
  { id: "Asia/Shanghai", label: "上海（中国标准时间）" },
  { id: "Asia/Hong_Kong", label: "香港" },
  { id: "Asia/Tokyo", label: "东京" },
  { id: "Asia/Singapore", label: "新加坡" },
  { id: "Europe/London", label: "伦敦" },
  { id: "Europe/Berlin", label: "柏林" },
  { id: "Europe/Paris", label: "巴黎" },
  { id: "America/New_York", label: "纽约" },
  { id: "America/Chicago", label: "芝加哥" },
  { id: "America/Los_Angeles", label: "洛杉矶" },
  { id: "UTC", label: "UTC" },
] as const;

export type TimezonePresetId = (typeof TIMEZONE_PRESETS)[number]["id"];

export function defaultTimezoneForPreset(
  preset: "balanced-zh-cn" | "balanced-en-us" | "balanced-de-de" | string,
): TimezonePresetId {
  switch (preset) {
    case "balanced-en-us":
      return "America/New_York";
    case "balanced-de-de":
      return "Europe/Berlin";
    default:
      return "Asia/Shanghai";
  }
}

export function isSupportedTimezone(value: string): value is TimezonePresetId {
  return TIMEZONE_PRESETS.some((preset) => preset.id === value);
}
