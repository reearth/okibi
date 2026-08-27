// The planner, through the binding.
//
// A service that runs a Worker can plan its own warming — which is worth
// doing only because it is the *same* planner. A second one would order the
// tiles slightly differently, warm somewhere slightly wrong, and never fail:
// the plan would still be a list of URLs with a cost beside it.
//
// So what these check is not that planning works — the Rust tests do that —
// but that the thing reachable from JavaScript is that planner, reads the
// documents the spec describes, and refuses what the planner refuses.

import { describe, expect, it } from "vitest";

import pricing from "../../../pricing/cloudflare-2026-08.json";
import { invalidationsBetween, plan } from "../nodejs/okibi.js";

const AT = "2026-08-24T02:00:00Z";

const epochs = (param: string) => ({
  service: "papers",
  tilesets: { "style-aoi-04": { source: "osm-2026-08-18", algo: "ezu-0.7.1", param } },
});

const manifest = {
  manifest: "okibi-service/1",
  service: "papers",
  url_template: "https://papers.reearth.land/t/{tileset}/{id}?e={epoch.param}",
  cost: {
    default_gen_ms: 30000,
    default_bytes: 90000,
    concurrency_limit: 4,
    rate_per_s: 2,
    billing: {
      pricing_profile: "cloudflare",
      per_gen: { cpu_ms: null, class_a_operation: 1, egress_byte: null },
    },
  },
  zoom_semantics: "resolution",
};


const digest = (req: number) => ({
  digest: "tile-demand-digest/1",
  service: "papers",
  tileset: "style-aoi-04",
  kind: "content",
  qk8: "13300211",
  window: "2026-08-23/P1D",
  req,
  miss: req,
  p50_gen_ms: 28900,
  p95_gen_ms: 41200,
  tiles_observed: 1,
  avg_bytes: 88231,
  top_qk: [["13300211231022", "14/14552/6451", req]],
});

describe("noticing what died", () => {
  it("reads a moved epoch as an invalidation", () => {
    const events = invalidationsBetween(epochs("r12"), epochs("r13"), AT, null);

    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      service: "papers",
      tileset: "style-aoi-04",
      axis: "param",
      epoch_from: "r12",
      epoch_to: "r13",
      occurred_at: AT,
    });
  });

  /// The reason a Worker can be the one asking: nothing moved is a thing it
  /// has to be able to say cheaply, every few hours, forever.
  it("says nothing when nothing moved", () => {
    expect(invalidationsBetween(epochs("r12"), epochs("r12"), AT, null)).toEqual([]);
  });
});

describe("planning", () => {
  it("orders the tiles and costs them", () => {
    const [invalidation] = invalidationsBetween(epochs("r12"), epochs("r13"), AT, null);

    const warm = plan({
      digests: [digest(1820)],
      invalidation,
      manifests: [manifest],
      pricing,
      epochs: epochs("r13"),
    });

    expect(warm.plan).toBe("okibi-warm-plan/1");
    expect(warm.entries.length).toBeGreaterThan(0);
    // The epochs come from the file rather than from the event, which is what
    // makes a URL right on every axis rather than only the one that moved.
    expect(warm.entries[0].url).toContain("e=r13");
    expect(warm.estimate.warm.usd).toBeGreaterThan(0);
  });

  /// A plan is stored, diffed and reviewed, so the same inputs have to give
  /// the same plan — and a caller in a Worker is a second place that has to
  /// be true of.
  it("gives the same plan twice", () => {
    const [invalidation] = invalidationsBetween(epochs("r12"), epochs("r13"), AT, null);
    const input = {
      digests: [digest(1820)],
      invalidation,
      manifests: [manifest],
      pricing,
      epochs: epochs("r13"),
    };

    expect(JSON.stringify(plan(input))).toBe(JSON.stringify(plan(input)));
  });

  /// Refusing here matters more than in the command line: a Worker's plan
  /// goes straight to the executor, with nobody reading it on the way.
  it("refuses a template whose epoch nobody supplied", () => {
    const [invalidation] = invalidationsBetween(epochs("r12"), epochs("r13"), AT, null);

    expect(() =>
      plan({
        digests: [digest(1820)],
        invalidation,
        manifests: [manifest],
        pricing,
        epochs: { service: "papers", tilesets: {} },
      }),
    ).toThrow(/epoch/);
  });

  it("refuses a service it has no manifest for", () => {
    const [invalidation] = invalidationsBetween(epochs("r12"), epochs("r13"), AT, null);

    expect(() =>
      plan({
        digests: [digest(1820)],
        invalidation,
        manifests: [],
        pricing,
        epochs: epochs("r13"),
      }),
    ).toThrow(/papers/);
  });
});
