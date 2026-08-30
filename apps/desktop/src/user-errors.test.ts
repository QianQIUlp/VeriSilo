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
    ).toBe("此 Silo 的浏览器数据正在使用中。请关闭对应浏览器后重试。");
    expect(userFacingErrorMessage("managed_network_mismatch")).toContain(
      "不会回退直连",
    );
  });
});
