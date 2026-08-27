import { describe, expect, it, vi } from "vitest";

import { authorised, readLimits } from "../src/index.js";
import {
  NotAPlan,
  WARM_HEADER,
  type WarmMessage,
  concurrencyFor,
  messagesFor,
  summarise,
  warmBatch,
} from "../src/plan.js";

const plan = (entries: unknown[]) => ({ plan: "okibi-warm-plan/1", entries });

describe("reading a plan", () => {
  it("takes one message per entry", () => {
    const messages = messagesFor(
      plan([
        { url: "https://a.test/1", service: "papers", priority: 1, lane: "warm" },
        { url: "https://a.test/2", service: "papers", priority: 0.5 },
      ]),
    );

    expect(messages).toEqual([
      { url: "https://a.test/1", service: "papers", lane: "warm" },
      { url: "https://a.test/2", service: "papers", lane: "warm" },
    ]);
  });

  it("keeps the lane the planner chose", () => {
    const [message] = messagesFor(
      plan([{ url: "https://a.test/1", service: "papers", lane: "urgent" }]),
    );
    expect(message?.lane).toBe("urgent");
  });

  /// A plan whose fields moved would be warmed wrong, and warming the wrong
  /// thing looks exactly like warming the right thing.
  it("refuses a version it does not read", () => {
    expect(() => messagesFor({ plan: "okibi-warm-plan/2", entries: [] })).toThrow(
      NotAPlan,
    );
    expect(() => messagesFor({ entries: [] })).toThrow(NotAPlan);
    expect(() => messagesFor("a plan, honest")).toThrow(NotAPlan);
  });

  it("refuses an entry it could not fetch", () => {
    expect(() => messagesFor(plan([{ service: "papers" }]))).toThrow(NotAPlan);
    expect(() => messagesFor(plan([{ url: "https://a.test/1" }]))).toThrow(
      NotAPlan,
    );
  });
});

describe("limits", () => {
  it("are the service's own, or the fallback, or four", () => {
    expect(concurrencyFor({ papers: 4, "*": 6 }, "papers")).toBe(4);
    expect(concurrencyFor({ papers: 4, "*": 6 }, "terrain")).toBe(6);
    expect(concurrencyFor({}, "terrain")).toBe(4);
  });

  it("are never zero, whatever the config says", () => {
    expect(concurrencyFor({ papers: 0 }, "papers")).toBe(1);
    expect(concurrencyFor({ papers: -3 }, "papers")).toBe(1);
  });

  it("fall back rather than fail when the config is malformed", () => {
    expect(readLimits(undefined)).toEqual({});
    expect(readLimits("not json")).toEqual({});
    expect(readLimits('{"papers":4}')).toEqual({ papers: 4 });
  });
});

describe("warming a batch", () => {
  const message = (url: string, service = "papers"): WarmMessage => ({
    url,
    service,
    lane: "warm",
  });

  it("marks every request as okibi's own", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response("", { status: 200 }));
    await warmBatch([message("https://a.test/1")], {}, "a-secret", fetcher as never);

    expect(fetcher).toHaveBeenCalledWith("https://a.test/1", {
      headers: { [WARM_HEADER]: "a-secret" },
    });
  });

  it("warms anyway when no secret is configured", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response("", { status: 200 }));
    await warmBatch([message("https://a.test/1")], {}, undefined, fetcher as never);

    expect(fetcher).toHaveBeenCalledWith("https://a.test/1", { headers: {} });
  });

  it("reports what did not warm without giving up on the rest", async () => {
    const fetcher = vi.fn(async (url: string) =>
      url.endsWith("2")
        ? new Response("", { status: 500 })
        : new Response("", { status: 200 }),
    );

    const outcomes = await warmBatch(
      [message("https://a.test/1"), message("https://a.test/2"), message("https://a.test/3")],
      {},
      "a-secret",
      fetcher as never,
    );

    expect(outcomes.map((o) => o.ok)).toEqual([true, false, true]);
    expect(outcomes[1]?.status).toBe(500);
  });

  it("survives a request that throws", async () => {
    const fetcher = vi.fn().mockRejectedValue(new Error("connection reset"));
    const [outcome] = await warmBatch([message("https://a.test/1")], {}, "a-secret", fetcher as never);

    expect(outcome?.ok).toBe(false);
    expect(outcome?.error).toBe("connection reset");
  });

  it("keeps within the service's concurrency", async () => {
    let inFlight = 0;
    let peak = 0;
    const fetcher = vi.fn(async () => {
      inFlight++;
      peak = Math.max(peak, inFlight);
      await new Promise((resolve) => setTimeout(resolve, 1));
      inFlight--;
      return new Response("", { status: 200 });
    });

    const messages = Array.from({ length: 10 }, (_, i) =>
      message(`https://a.test/${i}`),
    );
    await warmBatch(messages, { papers: 3 }, "a-secret", fetcher as never);

    expect(peak).toBeLessThanOrEqual(3);
    expect(fetcher).toHaveBeenCalledTimes(10);
  });

  /// Two origins in one batch should not have to take turns with each other.
  it("gives each service its own budget", async () => {
    const inFlight: Record<string, number> = { papers: 0, terrain: 0 };
    const peak: Record<string, number> = { papers: 0, terrain: 0 };

    const fetcher = vi.fn(async (url: string) => {
      const service = url.includes("papers") ? "papers" : "terrain";
      inFlight[service] = (inFlight[service] ?? 0) + 1;
      peak[service] = Math.max(peak[service] ?? 0, inFlight[service] ?? 0);
      await new Promise((resolve) => setTimeout(resolve, 1));
      inFlight[service] = (inFlight[service] ?? 0) - 1;
      return new Response("", { status: 200 });
    });

    await warmBatch(
      [
        ...Array.from({ length: 6 }, (_, i) => message(`https://papers.test/${i}`, "papers")),
        ...Array.from({ length: 6 }, (_, i) => message(`https://terrain.test/${i}`, "terrain")),
      ],
      { papers: 2, terrain: 5 },
      "a-secret",
      fetcher as never,
    );

    expect(peak.papers).toBeLessThanOrEqual(2);
    expect(peak.terrain).toBeLessThanOrEqual(5);
    expect(peak.terrain).toBeGreaterThan(2);
  });

  it("returns outcomes in the order it was given", async () => {
    const fetcher = vi.fn(async (url: string) => {
      // The last one answers first, which must not reorder the report.
      await new Promise((resolve) => setTimeout(resolve, url.endsWith("2") ? 0 : 5));
      return new Response("", { status: 200 });
    });

    const outcomes = await warmBatch(
      [message("https://a.test/0"), message("https://a.test/1"), message("https://a.test/2")],
      { papers: 3 },
      "a-secret",
      fetcher as never,
    );

    expect(outcomes.map((o) => o.url)).toEqual([
      "https://a.test/0",
      "https://a.test/1",
      "https://a.test/2",
    ]);
  });
});

describe("the plan endpoint", () => {
  const request = (token?: string) =>
    new Request("https://executor.test/plans", {
      method: "POST",
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });

  it("takes the right token and nothing else", () => {
    expect(authorised(request("secret"), "secret")).toBe(true);
    expect(authorised(request("wrong!"), "secret")).toBe(false);
    expect(authorised(request(), "secret")).toBe(false);
    expect(authorised(request("secret"), "")).toBe(false);
  });
});

describe("summarising a batch", () => {
  const message = (url: string, service: string): WarmMessage => ({
    url,
    service,
    lane: "warm",
  });

  /// The summary is what a log line carries, and a log that reads as a
  /// successful run of something that failed is worse than no log.
  it("counts what warmed and what did not, per service", () => {
    const messages = [
      message("https://a.test/1", "papers"),
      message("https://a.test/2", "papers"),
      message("https://b.test/1", "terrain"),
    ];
    const summary = summarise(messages, [
      { url: "https://a.test/1", ok: true, status: 200 },
      { url: "https://a.test/2", ok: false, status: 503 },
      { url: "https://b.test/1", ok: true, status: 200 },
    ]);

    expect(summary.warmed).toBe(2);
    expect(summary.failed).toBe(1);
    expect(summary.services).toEqual({
      papers: { warmed: 1, failed: 1 },
      terrain: { warmed: 1, failed: 0 },
    });
    expect(summary.statuses).toEqual({ "200": 2, "503": 1 });
  });

  /// A request that never got a status is not a request that got a zero.
  it("keeps a request that never reached an origin apart from one that did", () => {
    const summary = summarise(
      [message("https://a.test/1", "papers")],
      [{ url: "https://a.test/1", ok: false, error: "network" }],
    );

    expect(summary.statuses).toEqual({ error: 1 });
    expect(summary.failed).toBe(1);
  });

  it("says nothing happened when nothing did", () => {
    expect(summarise([], [])).toEqual({
      warmed: 0,
      failed: 0,
      services: {},
      statuses: {},
    });
  });
});
