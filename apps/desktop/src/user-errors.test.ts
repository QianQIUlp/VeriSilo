import { describe, expect, it } from "vitest";

import { UserFacingError, userFacingErrorMessage } from "./user-errors.js";

describe("user-facing errors", () => {
  it("only forwards messages explicitly written for the product UI", () => {
    expect(
      userFacingErrorMessage(new UserFacingError("请检查填写内容。")),
    ).toBe("请检查填写内容。");
    expect(userFacingErrorMessage(new Error("provider receipt mismatch"))).toBe(
      "操作没有完成。请检查当前设置后重试。",
    );
    expect(userFacingErrorMessage("raw native failure")).toBe(
      "操作没有完成。请检查当前设置后重试。",
    );
    expect(
      userFacingErrorMessage(
        "managed_profile_in_use: C:\\Users\\hidden\\profile",
      ),
    ).toBe("这个浏览器的数据正在使用。请先关掉对应窗口再试。");
    expect(userFacingErrorMessage("managed_network_mismatch")).toContain(
      "不会回退直连",
    );
    expect(userFacingErrorMessage("managed_browser_open_failed")).toContain(
      "没有打开成功",
    );
    expect(
      userFacingErrorMessage(
        "7897 是 Clash 给浏览器走流量的代理端口，不是读取代理组的控制端口。控制端口一般是 9097 或 9090。",
      ),
    ).toContain("9097");
    expect(
      userFacingErrorMessage({
        message:
          "本机没有可用的 Clash 控制口。Clash Verge 默认关闭 9097，请点「查找本机 Clash」或再点「读取代理组」，程序会走内核管道。",
      }),
    ).toContain("内核管道");
    expect(
      userFacingErrorMessage({
        message: "身份配置生成失败：ipwho.is observation failed",
      }),
    ).toContain("ipwho.is");
    expect(
      userFacingErrorMessage(
        "无法绑定所选 Mihomo 节点：Clash 当前是直连模式，所选节点不会生效。请在 Clash 里改回规则或全局模式后再启动。",
      ),
    ).toContain("直连模式");
  });
});
