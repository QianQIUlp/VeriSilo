export const TIMEZONE_PRESETS = [
  { id: "Asia/Shanghai", label: "上海（中国标准时间）" },
  { id: "Asia/Hong_Kong", label: "香港" },
  { id: "Asia/Tokyo", label: "东京" },
  { id: "Asia/Seoul", label: "首尔" },
  { id: "Asia/Singapore", label: "新加坡" },
  { id: "Asia/Kolkata", label: "加尔各答（印度标准时间）" },
  { id: "Asia/Manila", label: "马尼拉" },
  { id: "Europe/London", label: "伦敦" },
  { id: "Europe/Berlin", label: "柏林" },
  { id: "Europe/Paris", label: "巴黎" },
  { id: "Europe/Madrid", label: "马德里" },
  { id: "Europe/Rome", label: "罗马" },
  { id: "Europe/Moscow", label: "莫斯科" },
  { id: "Europe/Istanbul", label: "伊斯坦布尔" },
  { id: "Africa/Cairo", label: "开罗" },
  { id: "America/New_York", label: "纽约" },
  { id: "America/Chicago", label: "芝加哥" },
  { id: "America/Los_Angeles", label: "洛杉矶" },
  { id: "America/Toronto", label: "多伦多" },
  { id: "America/Sao_Paulo", label: "圣保罗" },
  { id: "Australia/Sydney", label: "悉尼" },
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
    case "balanced-ja-jp":
      return "Asia/Tokyo";
    case "balanced-ko-kr":
      return "Asia/Seoul";
    case "balanced-en-gb":
      return "Europe/London";
    case "balanced-fr-fr":
      return "Europe/Paris";
    case "balanced-es-es":
      return "Europe/Madrid";
    case "balanced-it-it":
      return "Europe/Rome";
    case "balanced-ru-ru":
      return "Europe/Moscow";
    case "balanced-pt-br":
      return "America/Sao_Paulo";
    case "balanced-en-ca":
      return "America/Toronto";
    case "balanced-en-au":
      return "Australia/Sydney";
    case "balanced-en-in":
      return "Asia/Kolkata";
    case "balanced-en-sg":
      return "Asia/Singapore";
    case "balanced-en-ph":
      return "Asia/Manila";
    case "balanced-tr-tr":
      return "Europe/Istanbul";
    case "balanced-ar-eg":
      return "Africa/Cairo";
    default:
      return "Asia/Shanghai";
  }
}

export function isSupportedTimezone(value: string): value is TimezonePresetId {
  return TIMEZONE_PRESETS.some((preset) => preset.id === value);
}
