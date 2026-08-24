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
}

export interface Writer {
  /** Write one tile request. Never throws. */
  write(demand: TileDemand): void;
  /** The same, as the event it would write. Throws if the event is not valid. */
  eventFor(demand: TileDemand): TileDemandEvent;
}

/**
 * A writer bound to one service's dataset and epochs.
 *
 * A service supplies what only it knows — which tile, whether it hit, how long
 * it took. The service name and the epochs come from `okibi.epochs.json`,
 * which is also where its cache keys come from, so the two cannot disagree.
 */
export function createWriter({
  dataset,
  epochs,
  onError = (error) => console.error("okibi:", error),
}: WriterOptions): Writer {
  const eventFor = (demand: TileDemand): TileDemandEvent => ({
    ...demand,
    service: epochs.service,
    epoch: epochFor(epochs, demand.tileset),
  });

  return {
    eventFor,
    write(demand) {
      try {
        dataset.writeDataPoint(toDataPoint(eventFor(demand)));
      } catch (error) {
        onError(error, demand);
      }
    },
  };
}
