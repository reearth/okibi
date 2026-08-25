import { describe, expect, it } from "vitest";

import { NotWritable, check, qk8, toDataPoint } from "../writer/wae.js";
import type { TileDemandEvent } from "../writer/types.js";

const papers: TileDemandEvent = {
  service: "papers",
  tileset: "style-aoi-04",
  kind: "content",
  id: "14/14552/6451",
  qk: "13300211231022",
  cacheStatus: "miss",
  epoch: {
    source: "osm-2026-08-18",
    algo: "ezu-0.7.1",
    param: "style-aoi-04@r13",
  },
  fmt: "png",
  origin: "organic",
  genMs: 34120,
  bytes: 88231,
  z: 14,
};

describe("the wae-1 column order", () => {
  it("puts every attribute where the binding says", () => {
    const point = toDataPoint({
      ...papers,
      colo: "NRT",
      genDepMs: 120,
      cacheStatus: "hit",
      genMs: 0,
      cacheLayer: "edge",
    });

    // blob1..blob14, in the order spec/bindings/wae-1.md gives them.
    expect(point.blobs).toEqual([
      "papers",
      "style-aoi-04",
      "content",
      "14/14552/6451",
      "13300211231022",
      "13300211",
      "hit",
      "osm-2026-08-18",
      "ezu-0.7.1",
      "style-aoi-04@r13",
      "png",
      "NRT",
      "organic",
      "edge",
    ]);

    // double1..double5.
    expect(point.doubles).toEqual([1, 0, 120, 88231, 14]);

    // The index is the service alone, which is the unit sampling is applied
    // within.
    expect(point.indexes).toEqual(["papers"]);
  });

  it("counts one per request, whatever the request was", () => {
    expect(toDataPoint(papers).doubles[0]).toBe(1);
    expect(
      toDataPoint({ ...papers, cacheStatus: "hit", genMs: 0 }).doubles[0],
    ).toBe(1);
  });

  it("fills the optional columns rather than leaving holes", () => {
    const point = toDataPoint(papers);
    expect(point.blobs[11]).toBe(""); // colo
    expect(point.doubles[2]).toBe(0); // genDepMs
  });

  it("carries a request with no coordinates without pretending it has any", () => {
    const point = toDataPoint({
      ...papers,
      kind: "tileset",
      id: "tileset.json",
      qk: undefined,
      z: undefined,
      cacheStatus: "hit",
      genMs: 0,
      fmt: "json",
    });

    expect(point.blobs[4]).toBe(""); // qk
    expect(point.blobs[5]).toBe(""); // qk8
    expect(point.doubles[4]).toBe(-1); // z
  });
});

describe("qk8", () => {
  it("is the first eight characters", () => {
    expect(qk8("13300211231022")).toBe("13300211");
  });

  it("is the whole thing when there is less than that", () => {
    expect(qk8("1330")).toBe("1330");
  });
});

describe("what will not be written", () => {
  it("refuses a content event that cannot be placed", () => {
    expect(() => check({ ...papers, qk: undefined })).toThrow(NotWritable);
    expect(() => check({ ...papers, z: undefined })).toThrow(NotWritable);
  });

  it("refuses a hit that claims to have generated something", () => {
    expect(() => check({ ...papers, cacheStatus: "hit" })).toThrow(NotWritable);
    expect(() =>
      check({ ...papers, cacheStatus: "hit", genMs: 0 }),
    ).not.toThrow();
  });
});

/// An epoch is a part of the cache key, and a service whose key has two parts
/// has two. What it may not have is none: such an event can never be matched
/// against an invalidation, so it would be written, counted, and unusable.
describe("as many epochs as the key has", () => {
  it("takes two where there are two", () => {
    const point = toDataPoint({
      ...papers,
      epoch: { source: "2026-08-24", algo: "9005" },
    });

    expect(point.blobs.slice(7, 10)).toEqual(["2026-08-24", "9005", ""]);
  });

  it("refuses an event with no epoch at all", () => {
    expect(() => check({ ...papers, epoch: {} })).toThrow(NotWritable);
    expect(() =>
      check({ ...papers, epoch: { source: "", algo: "", param: "" } }),
    ).toThrow(NotWritable);
  });
});

/// A hit is a hit for warming, whichever layer had it. It is not a hit for
/// billing: an edge hit costs nothing and a read from an object store is a
/// priced operation, and without the distinction the cost of serving is a
/// range rather than a number.
describe("which layer answered", () => {
  it("goes in the column after origin", () => {
    const point = toDataPoint({
      ...papers,
      cacheStatus: "hit",
      genMs: 0,
      cacheLayer: "store",
    });

    expect(point.blobs[12]).toBe("organic");
    expect(point.blobs[13]).toBe("store");
  });

  it("is an empty column when nothing says", () => {
    expect(toDataPoint(papers).blobs[13]).toBe("");
  });

  it("refuses a miss that claims a layer answered it", () => {
    expect(() => check({ ...papers, cacheStatus: "miss", cacheLayer: "edge" })).toThrow(
      NotWritable,
    );
  });
});
