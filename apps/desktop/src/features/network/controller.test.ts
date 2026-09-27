import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  type MihomoControllerError,
  type MihomoSnapshot,
} from "../../desktop-api.js";
import { UserFacingError, userFacingErrorMessage } from "../../user-errors.js";
import { readMihomoGroups } from "./controller.js";

const direct = "http://127.0.0.1:9097/";
const discovered = "http://127.0.0.1:9090/";
const snapshot: MihomoSnapshot = {
  checkedAt: "2026-09-27T00:00:00Z",
  groups: [],
  providers: [],
  controllerUrl: discovered,
};

describe("Clash controller rediscovery", () => {
  beforeEach(() => invokeMock.mockReset());

  it.each([
    "controller_unreachable",
    "controller_transport",
    "mixed_port",
  ] as const)(
    "rediscovers for %s independently of the message wording",
    async (code) => {
      const failure: MihomoControllerError = {
        code,
        message: "changed or translated copy",
      };
      invokeMock
        .mockRejectedValueOnce(failure)
        .mockResolvedValueOnce({ controllerUrl: discovered })
        .mockResolvedValueOnce(snapshot);

      await expect(readMihomoGroups(direct, "secret")).resolves.toEqual({
        snapshot,
        controllerUrl: discovered,
      });
      expect(invokeMock.mock.calls).toEqual([
        [
          "inspect_mihomo_controller",
          { input: { controllerUrl: direct, secret: "secret" } },
        ],
        ["probe_local_clash", { secret: "secret" }],
        [
          "inspect_mihomo_controller",
          { input: { controllerUrl: discovered, secret: "secret" } },
        ],
      ]);
    },
  );

  it.each([
    {
      code: "controller_rejected",
      message: "本机 Mihomo Controller 返回 HTTP 401；请检查地址和 Secret。",
    },
    {
      code: "controller_rejected",
      message: "无法连接本机：此认证错误仍不能切换目标。",
    },
    { code: "controller_internal", message: "无法连接本机" },
    { code: "unknown", message: "没有可用的 Clash 控制口" },
    "无法连接本机 Mihomo Controller：积极拒绝",
    { code: "controller_transport" },
  ])(
    "keeps the selected target for a rejected or untyped failure: %j",
    async (failure) => {
      invokeMock.mockRejectedValueOnce(failure);
      await expect(readMihomoGroups(direct, "secret")).rejects.toBe(failure);
      expect(invokeMock).toHaveBeenCalledTimes(1);
    },
  );

  it("preserves existing authentication copy", async () => {
    const failure: MihomoControllerError = {
      code: "controller_rejected",
      message: "本机 Mihomo Controller 返回 HTTP 403；请检查地址和 Secret。",
    };
    expect(userFacingErrorMessage(failure)).toBe(failure.message);
  });

  it("returns a successful direct inspection without probing", async () => {
    const directSnapshot = { ...snapshot, controllerUrl: direct };
    invokeMock.mockResolvedValueOnce(directSnapshot);
    await expect(readMihomoGroups(direct, "secret")).resolves.toEqual({
      snapshot: directSnapshot,
      controllerUrl: direct,
    });
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("keeps the direct transport error as detail when discovery finds no controller", async () => {
    const failure: MihomoControllerError = {
      code: "controller_transport",
      message: "无法连接本机 Mihomo Controller：连接被拒绝。",
    };
    invokeMock
      .mockRejectedValueOnce(failure)
      .mockResolvedValueOnce({
        controllerUrl: null,
        detail: "请先打开 Clash。",
      });
    const error = await readMihomoGroups(direct, "secret").catch(
      (failure: unknown) => failure,
    );
    expect(error).toBeInstanceOf(UserFacingError);
    expect(error).toMatchObject({
      message: "请先打开 Clash。",
      detail: failure.message,
    });
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("does not retry an authentication failure on the discovered controller", async () => {
    const failure: MihomoControllerError = {
      code: "controller_rejected",
      message: "本机 Mihomo Controller 返回 HTTP 401；请检查地址和 Secret。",
    };
    invokeMock
      .mockRejectedValueOnce({
        code: "controller_unreachable",
        message: "offline",
      })
      .mockResolvedValueOnce({ controllerUrl: discovered })
      .mockRejectedValueOnce(failure);
    await expect(readMihomoGroups(direct, "secret")).rejects.toBe(failure);
    expect(invokeMock).toHaveBeenCalledTimes(3);
  });
});
