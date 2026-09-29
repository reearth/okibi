import { type EpochsFile, epochFor } from "./epochs.js";
import type { TileDemand, TileDemandEvent } from "./types.js";
import { type Dataset, toDataPoint } from "./wae.js";

export interface WriterOptions {
  /** The dataset binding to write to. */
  dataset: Dataset;
  /** The service's `okibi.epochs.json`, imported. */
  epochs: EpochsFile;
  /**
   * Where a refused or failed write goes.
   *
   * Writing happens on the response path, so a throw here would turn a
   * bookkeeping problem into a failed request. The ledger is worth a great
   * deal and no single tile response is worth losing for it, so problems are
   * reported rather than raised. Defaults to `console.error`.
   */
  onError?: (error: unknown, demand: TileDemand) => void;
  /**
   * Write one organic hit in this many, weighted to stand for the rest.
   * Defaults to `1`, which writes every one.
   *
   * For a service whose ledger is mostly hits, and large enough that paying
   * for every one of them is a decision. Totals stay right, because the one
   * that is written carries the weight of the ones that are not. What is lost
   * is the tail: a tile with `n` hits is counted to within about
   * `sqrt((k - 1) / n)`, and one hit fewer than `k` times may not appear at
   * all. Misses, stale answers, errors and warm requests are always written —
   * see `spec/tile-demand.md` for why each of them has to be.
   */
  sampleHits?: number;
  /**
   * A number in `[0, 1)`, as `Math.random` gives. What decides which hits
   * are kept, and here so that a test can decide it instead.
   */
  random?: () => number;
}

export interface Writer {
  /** Write one tile request. Never throws. */
  write(demand: TileDemand): void;
  /**
   * The same, as the event it would write if it were written unsampled.
   * Throws if the event is not valid.
   */
  eventFor(demand: TileDemand): TileDemandEvent;
}

/**
 * A writer bound to one service's dataset and epochs.
 *
 * A service supplies what only it knows — which tile, whether it hit, how long
 * it took. The service name and the epochs come from `okibi.epochs.json`,
 * which is also where its cache keys come from, so the two cannot disagree.
 *
 * Throws if `sampleHits` is not a whole number of at least one. That is a
 * mistake in how the writer was set up rather than in any one request, and it
 * is better found when the Worker starts than as a ledger with nothing in it.
 */
export function createWriter({
  dataset,
  epochs,
  onError = (error) => console.error("okibi:", error),
  sampleHits = 1,
  random = Math.random,
}: WriterOptions): Writer {
  if (!Number.isInteger(sampleHits) || sampleHits < 1) {
    throw new RangeError(
      `sampleHits is the k in "one hit in k", a whole number of at least 1 — got ${sampleHits}`,
    );
  }

  const eventFor = (demand: TileDemand): TileDemandEvent => ({
    ...demand,
    service: epochs.service,
    // What the caller says it cached under, or what the file says. The file
    // is the usual answer and the better one — there is only one string, so
    // there is nothing to drift from — but a service that resolves part of
    // its key at request time knows something the file cannot.
    epoch: demand.epoch ?? epochFor(epochs, demand.tileset),
  });

  const sampled = (demand: TileDemand): boolean =>
    sampleHits > 1 && demand.cacheStatus === "hit" && demand.origin === "organic";

  return {
    eventFor,
    write(demand) {
      try {
        const event = eventFor(demand);
        const sampling = sampled(demand);
        const point = toDataPoint(sampling ? { ...event, count: sampleHits } : event);
        // Decided after the event is built and checked rather than before,
        // so that a service sending a malformed hit hears about every one of
        // them and not only the ones that happened to be kept.
        if (sampling && random() >= 1 / sampleHits) {
          return;
        }
        dataset.writeDataPoint(point);
      } catch (error) {
        onError(error, demand);
      }
    },
  };
}
