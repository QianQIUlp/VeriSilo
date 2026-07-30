import { describe, expect, it } from "vitest";

import { parseProxyInput } from "./proxy-input.js";

describe("proxy input parser", () => {
  it("parses a common host:port:user:password line and requires the proxy", () => {
    expect(parseProxyInput("proxy.example.test:8080:alice:s3cret")).toEqual({
      profile: {
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: "http",
        host: "proxy.example.test",
        port: 8080,
        bypassList: [],
      },
      credentials: { username: "alice", password: "s3cret" },
    });
  });

  it("parses a standard authenticated SOCKS5 URL", () => {
    expect(
      parseProxyInput("socks5://name:p%40ss@127.0.0.1:7890"),
    ).toMatchObject({
      profile: { scheme: "socks5", host: "127.0.0.1", port: 7890 },
      credentials: { username: "name", password: "p@ss" },
    });
  });

  it("rejects embedded paths, missing paired credentials, and invalid ports", () => {
    expect(() =>
      parseProxyInput("http://proxy.example.test:8080/config"),
    ).toThrow(/只能包含/);
    expect(() => parseProxyInput("proxy.example.test:80:user:")).toThrow(
      /同时填写/,
    );
    expect(() => parseProxyInput("proxy.example.test:70000")).toThrow(/65535/);
  });
});
