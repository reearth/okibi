/**
 * The `wae-1` binding: where each attribute physically lands in Workers
 * Analytics Engine. See `spec/bindings/wae-1.md`.
 *
 * The column order exists here and nowhere else. Three services packing it
 * themselves would be three chances to shift a blob by one and produce a
 * ledger that looks fine and means something different.
 */

import type { TileDemandEvent } from "./types.js";

/** The dataset binding, as much of it as the writer uses. */
export interface Dataset {
  writeDataPoint(point: DataPoint): void;
}

export interface DataPoint {
  indexes: string[];
  blobs: string[];
  doubles: number[];
}

export class NotWritable extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NotWritable";
  }
}

/** `tile.qk8`: the first eight characters, or all of them if there are fewer. */
export function qk8(qk: string): string {
  return qk.slice(0, 8);
}

/**
 * Refuses an event the vocabulary would not accept.
 *
 * A `content` event with no `qk` is not a slightly worse record — it is a
 * request that cannot be placed anywhere, and every aggregate over the cell it
 * belonged in is wrong by however many of these there were.
 */
export function check(event: TileDemandEvent): void {
  if (event.kind === "content") {
    if (!event.qk) {
      throw new NotWritable(`content event ${event.id} has no qk`);
    }
    if (event.z === undefined) {
      throw new NotWritable(`content event ${event.id} has no z`);
    }
  }
  if (event.cacheStatus === "miss" && event.cacheLayer) {
    throw new NotWritable(
      `${event.id} missed, so no layer answered it — got ${event.cacheLayer}`,
    );
  }
  if (event.cacheStatus === "hit" && event.genMs !== 0) {
    throw new NotWritable(
      `hit for ${event.id} claims ${event.genMs}ms of generation`,
    );
  }
  // An event with no epoch at all can never be matched against an
  // invalidation, so it would aggregate into a cell no plan could ever act
  // on: written, counted, and unusable.
  const { source, algo, param } = event.epoch;
  if (!source && !algo && !param) {
    throw new NotWritable(`${event.id} has no epoch to have been cached under`);
  }
}

/** One event as Analytics Engine columns. */
export function toDataPoint(event: TileDemandEvent): DataPoint {
  check(event);

  const qk = event.qk ?? "";
  return {
    indexes: [event.service],
    blobs: [
      event.service,
      event.tileset,
      event.kind,
      event.id,
      qk,
      qk ? qk8(qk) : "",
      event.cacheStatus,
      // An axis this service does not have is an empty column, not a missing
      // one: the binding is positional, and a hole would shift everything
      // after it into the wrong blob.
      event.epoch.source ?? "",
      event.epoch.algo ?? "",
      event.epoch.param ?? "",
      event.fmt,
      event.colo ?? "",
      event.origin,
      event.cacheLayer ?? "",
    ],
    doubles: [
      1,
      event.genMs,
      event.genDepMs ?? 0,
      event.bytes,
      event.z ?? -1,
    ],
  };
}
