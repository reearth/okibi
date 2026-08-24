// The wasm binding, actually run.
//
// The Rust side is thoroughly tested, but a binding that fails to export what
// it claims, or exports it under another name, passes every Rust test there
// is. This imports the built package the way a service will.

import { describe, expect, it } from "vitest";

import { qk8, quadkeyForPoint, quadkeyForTile, startsWith } from "../nodejs/okibi.js";

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
