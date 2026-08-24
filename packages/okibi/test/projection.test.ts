// The wasm binding, actually run.
//
// The Rust side is thoroughly tested, but a binding that fails to export what
// it claims, or exports it under another name, passes every Rust test there
// is. This imports the built package the way a service will.

import { describe, expect, it } from "vitest";

import {
  assembleDigest,
  digestQueries,
  qk8,
  quadkeyForPoint,
  quadkeyForTile,
  startsWith,
} from "../nodejs/okibi.js";

describe("projection through the binding", () => {
  /// The claim the vocabulary rests on: three services numbering the same
  /// ground three different ways land in one digest cell.
  it("puts Tokyo in one cell whatever the scheme", () => {
    const papers = quadkeyForTile("web-mercator", 14, 14552, 6451);
    const buildings = quadkeyForTile("web-mercator", 13, 7276, 3225);
    const terrain = quadkeyForTile("geographic-tms", 14, 29108, 11439);

    expect(qk8(papers)).toBe("13300211");
    expect(qk8(buildings)).toBe("13300211");
    expect(qk8(terrain)).toBe("13300211");
  });

  it("agrees with the published example", () => {
    // Microsoft's own tile-system documentation: (3, 3, 5) is "213".
    expect(quadkeyForTile("web-mercator", 3, 3, 5)).toBe("213");
  });

  it("projects a point to the tile containing it", () => {
    const fromPoint = quadkeyForPoint(139.7671, 35.6812, 14);
    expect(qk8(fromPoint)).toBe("13300211");
  });

  it("matches an invalidation scope by prefix", () => {
    const tile = quadkeyForTile("web-mercator", 14, 14552, 6451);
    expect(startsWith(tile, "133002")).toBe(true);
    expect(startsWith(tile, "133003")).toBe(false);
  });

  it("refuses a tile that is not on the grid", () => {
    expect(() => quadkeyForTile("web-mercator", 2, 4, 0)).toThrow();
    expect(() => quadkeyForPoint(181, 0, 8)).toThrow();
    expect(() => qk8("1334")).toThrow();
  });
});

describe("the digest, through the binding", () => {
  it("asks for the day it was given", () => {
    const { cells, topTiles } = digestQueries(
      { services: ["papers"], top_rows: 500 },
      "2026-08-23",
    );

    expect(cells).toContain("FROM tile_demand_1");
    expect(cells).toContain("timestamp >= toDateTime('2026-08-23 00:00:00')");
    expect(cells).toContain("timestamp < toDateTime('2026-08-24 00:00:00')");
    expect(cells).toContain("index1 IN ('papers')");
    expect(topTiles).toContain("LIMIT 500");
  });

  /// The two rules that do not fail visibly when they are left out.
  it("weighs every frequency and counts organic demand only", () => {
    const { cells } = digestQueries(undefined, "2026-08-23");

    for (const line of cells.split("\n")) {
      if (line.includes("double1") || line.includes("double4")) {
        expect(line, line).toContain("_sample_interval");
      }
    }
    expect(cells).toContain("IF(blob13 = 'organic'");
  });

  it("reads every service when none are named", () => {
    expect(digestQueries(undefined, "2026-08-23").cells).not.toContain("index1");
  });

  it("rolls rows up the way the planner will read them", () => {
    const { records, skipped } = assembleDigest(
      [
        {
          service: "papers",
          tileset: "style-aoi-04",
          kind: "content",
          qk8: "13300211",
          req: 48210,
          miss: "312",
          p50_gen_ms: 28900,
          tiles_observed: "1240",
        },
      ],
      [
        { service: "papers", tileset: "style-aoi-04", kind: "content", qk8: "13300211",
          qk: "13300211231023", id: "14/14553/6451", req: 1544 },
        { service: "papers", tileset: "style-aoi-04", kind: "content", qk8: "13300211",
          qk: "13300211231022", id: "14/14552/6451", req: 1820 },
      ],
      "2026-08-23",
      20,
    );

    expect(records).toHaveLength(1);
    expect(records[0].digest).toBe("tile-demand-digest/1");
    expect(records[0].window).toBe("2026-08-23/P1D");
    // Numbers arrive as numbers or as strings depending on the aggregate that
    // produced them, and both are the same number.
    expect(records[0].tiles_observed).toBe(1240);
    expect(records[0].miss).toBe(312);
    // Hottest first, carrying both the quadkey and the id a URL needs.
    expect(records[0].top_qk).toEqual([
      ["13300211231022", "14/14552/6451", 1820],
      ["13300211231023", "14/14553/6451", 1544],
    ]);
    expect(skipped).toEqual({ unknown_kind: 0, unplaceable: 0, cells_without_top: 0 });
  });

  it("reports a row it could not place rather than dropping it", () => {
    const { records, skipped } = assembleDigest(
      [{ service: "papers", tileset: "t", kind: "content", qk8: "", req: 1, miss: 0,
         tiles_observed: 1 }],
      [],
      "2026-08-23",
      20,
    );

    expect(records).toHaveLength(0);
    expect(skipped.unplaceable).toBe(1);
  });

  /// A sampled cell keeps the right totals and loses the tail, so how hard it
  /// was sampled is worth carrying to whoever wonders why an estimate missed.
  it("carries how hard the rows were sampled", () => {
    const { records } = assembleDigest(
      [
        {
          service: "papers",
          tileset: "t",
          kind: "content",
          qk8: "13300211",
          req: 90000,
          miss: 0,
          tiles_observed: 40,
          sample_interval_max: 100,
        },
      ],
      [],
      "2026-08-23",
      20,
    );

    expect(records[0].sample_interval_max).toBe(100);
  });

  it("asks the backend for it", () => {
    expect(digestQueries(undefined, "2026-08-23").cells).toContain(
      "MAX(_sample_interval) AS sample_interval_max",
    );
  });
});
