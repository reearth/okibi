import { describe, expect, it, vi } from "vitest";

import { UnknownTileset, cacheKeyFor, epochFor } from "../writer/epochs.js";
import type { EpochsFile } from "../writer/epochs.js";
import { WARM_HEADER, originOf, warmHeaders } from "../writer/origin.js";
import type { TileDemand } from "../writer/types.js";
import type { DataPoint } from "../writer/wae.js";
import { createWriter } from "../writer/writer.js";

const epochs: EpochsFile = {
  service: "papers",
  tilesets: {
    "style-aoi-04": {
      source: "osm-2026-08-18",
      algo: "ezu-0.7.1",
      param: "style-aoi-04@r13",
    },
  },
};

const demand: TileDemand = {
  tileset: "style-aoi-04",
  kind: "content",
  id: "14/14552/6451",
  qk: "13300211231022",
  cacheStatus: "miss",
  fmt: "png",
  origin: "organic",
  genMs: 34120,
  bytes: 88231,
  z: 14,
};

function sink() {
  const written: DataPoint[] = [];
  return { written, writeDataPoint: (p: DataPoint) => void written.push(p) };
}

describe("the writer", () => {
  it("fills in what the service should not be repeating", () => {
    const dataset = sink();
    createWriter({ dataset, epochs }).write(demand);

    const [point] = dataset.written;
    expect(point?.blobs[0]).toBe("papers");
    expect(point?.blobs.slice(7, 10)).toEqual([
      "osm-2026-08-18",
      "ezu-0.7.1",
      "style-aoi-04@r13",
    ]);
  });

  /// The epochs in an event and the epochs in a cache key are the same
  /// strings, read once. This is the property spec/tile-demand.md asks for and
  /// the reason the file exists at all.
  it("writes the epochs the cache key was built from", () => {
    const dataset = sink();
    createWriter({ dataset, epochs }).write(demand);

    const [point] = dataset.written;
    expect(point?.blobs.slice(7, 10).join("/")).toBe(
      cacheKeyFor(epochs, "style-aoi-04"),
    );
  });

  /// Writing happens as the response goes out, so a bad event must not be
  /// able to take the response with it.
  it("reports a refused write rather than raising it", () => {
    const dataset = sink();
    const onError = vi.fn();

    const writer = createWriter({ dataset, epochs, onError });
    expect(() => writer.write({ ...demand, qk: undefined })).not.toThrow();

    expect(dataset.written).toHaveLength(0);
    expect(onError).toHaveBeenCalledOnce();
  });

  it("reports an unknown tileset the same way", () => {
    const dataset = sink();
    const onError = vi.fn();

    createWriter({ dataset, epochs, onError }).write({
      ...demand,
      tileset: "style-aoi-99",
    });

    expect(onError).toHaveBeenCalledOnce();
    expect(onError.mock.calls[0]?.[0]).toBeInstanceOf(UnknownTileset);
  });

  it("hands back the event it would write, for anyone who wants to look", () => {
    const dataset = sink();
    const event = createWriter({ dataset, epochs }).eventFor(demand);

    expect(event.service).toBe("papers");
    expect(event.epoch).toEqual(epochFor(epochs, "style-aoi-04"));
  });
});

describe("where a request came from", () => {
  const SECRET = "a-shared-secret";
  const request = (headers: Record<string, string>) => ({
    headers: { get: (name: string) => headers[name] ?? null },
  });

  it("is warm when okibi asked", () => {
    expect(originOf(request(warmHeaders(SECRET)), SECRET)).toBe("warm");
    expect(warmHeaders(SECRET)).toEqual({ [WARM_HEADER]: SECRET });
  });

  it("is organic when anyone else did", () => {
    expect(originOf(request({}), SECRET)).toBe("organic");
    expect(originOf(request({ "User-Agent": "curl" }), SECRET)).toBe("organic");
  });

  /// Demand that is not recorded is demand that is never warmed, so a mark
  /// anyone could send would let anyone quietly delete their own traffic
  /// from the ledger.
  it("is organic when the mark is forged", () => {
    expect(originOf(request({ [WARM_HEADER]: "1" }), SECRET)).toBe("organic");
    expect(originOf(request({ [WARM_HEADER]: "guessing" }), SECRET)).toBe("organic");
    expect(originOf(request({ [WARM_HEADER]: SECRET.slice(0, -1) }), SECRET)).toBe(
      "organic",
    );
  });

  /// Erring toward counting okibi's own traffic as demand is bounded and
  /// visible; the other direction is a ledger anyone can edit.
  it("marks nothing when no secret is configured", () => {
    expect(originOf(request({ [WARM_HEADER]: "anything" }), undefined)).toBe("organic");
    expect(originOf(request({ [WARM_HEADER]: "anything" }), "")).toBe("organic");
  });
});
