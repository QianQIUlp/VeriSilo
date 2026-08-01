import { describe, expect, it } from "vitest";

import { networkCheckErrorMessage } from "./network-check-client.js";

describe("desktop network-check errors", () => {
  it("maps failures without exposing native error text", () => {
    expect(
      networkCheckErrorMessage(new Error("request to /private failed")),
    ).toBe("请求失败");
    expect(
      networkCheckErrorMessage(new SyntaxError("raw parser details")),
    ).toBe("返回内容无法识别");
    expect(
      networkCheckErrorMessage(new DOMException("raw timeout", "AbortError")),
    ).toBe("请求超时");
  });
});
