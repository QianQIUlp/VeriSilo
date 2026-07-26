import { describe, expect, it } from "vitest";

import {
  buildNetworkCheckResult,
  isNetworkCheckResult,
  parseIpExit,
} from "./network-check.js";

const ipPayload = {
  ip: "203.0.113.10",
  success: true,
  type: "IPv4",
  country: "Singapore",
  country_code: "SG",
  region: "Singapore",
  city: "Singapore",
  connection: {
    asn: 14_061,
    org: "DigitalOcean, LLC",
    isp: "DigitalOcean, LLC",
  },
  timezone: { id: "Asia/Singapore" },
};

const cloudflareDnsPayload = {
  Status: 0,
  AD: true,
  Answer: [
    { type: 1, data: "172.66.147.243" },
    { type: 1, data: "104.20.23.154" },
    { type: 46, data: "signature" },
  ],
};

const googleDnsPayload = {
  Status: 0,
  AD: true,
  Answer: [
    { type: 1, data: "104.20.23.154" },
    { type: 1, data: "172.66.147.243" },
  ],
};

describe("network check parsing", () => {
  it("extracts bounded exit information and labels only a hosting hint", () => {
    expect(parseIpExit(ipPayload)).toMatchObject({
      address: "203.0.113.10",
      countryCode: "SG",
      asn: "AS14061",
      organization: "DigitalOcean, LLC",
      timezone: "Asia/Singapore",
      networkHint: "cloud_or_hosting",
    });
  });

  it("reports public DoH agreement without inventing an IP purity score", () => {
    const result = buildNetworkCheckResult({
      ipPayload,
      cloudflareDnsPayload,
      googleDnsPayload,
      checkedAt: "2026-07-26T00:00:00.000Z",
    });
    expect(result.dns.state).toBe("consistent");
    expect(result.dns.dnssec).toBe("validated");
    expect(result.reputation.state).toBe("not_scored");
    expect(isNetworkCheckResult(result)).toBe(true);
  });

  it("rejects malformed remote payloads instead of rendering them", () => {
    expect(parseIpExit({ success: true, ip: "x".repeat(1_000) })).toBeNull();
    expect(
      isNetworkCheckResult({
        schemaVersion: 1,
        checkedAt: "2026-07-26T00:00:00.000Z",
        dns: {},
        reputation: {},
        errors: [],
      }),
    ).toBe(false);
  });
});
