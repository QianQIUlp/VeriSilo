import { type MihomoSnapshot, desktopApi } from "../../desktop-api.js";

import {
  isClashPipeController,
  isCommonClashMixedPort,
} from "../../proxy-presets.js";

import { UserFacingError, userFacingErrorMessage } from "../../user-errors.js";

export async function readMihomoGroups(
  controllerUrl: string,
  secret: string,
): Promise<{ snapshot: MihomoSnapshot; controllerUrl: string }> {
  const inspect = async (url: string) => {
    const snapshot = await desktopApi.inspectMihomoController({
      controllerUrl: url,
      secret,
    });
    return {
      snapshot,
      controllerUrl: snapshot.controllerUrl ?? url,
    };
  };
  const trimmed = controllerUrl.trim();
  let canTryDirect = false;
  if (trimmed !== "") {
    if (isClashPipeController(trimmed)) {
      canTryDirect = true;
    } else {
      try {
        const parsed = new URL(trimmed);
        const port = Number(parsed.port);
        canTryDirect =
          parsed.protocol === "http:" &&
          parsed.hostname === "127.0.0.1" &&
          Number.isInteger(port) &&
          port > 0 &&
          !isCommonClashMixedPort(port);
      } catch {
        canTryDirect = false;
      }
    }
  }
  if (canTryDirect) {
    try {
      return await inspect(trimmed);
    } catch (error) {
      const text = userFacingErrorMessage(error);
      if (
        !text.includes("没有可用的 Clash 控制口") &&
        !text.includes("无法连接本机") &&
        !text.includes("积极拒绝") &&
        !text.includes("是 Clash 给浏览器走流量的代理端口")
      ) {
        throw error;
      }
    }
  }
  const probe = await desktopApi.probeLocalClash(secret);
  if (probe.controllerUrl) {
    return inspect(probe.controllerUrl);
  }
  throw new UserFacingError(
    probe.detail ||
      "本机没有可用的 Clash 控制口。请先打开 Clash Verge / Mihomo。",
  );
}
