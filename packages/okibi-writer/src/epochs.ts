import type { Epoch } from "./types.js";

/**
 * The contents of a service's `okibi.epochs.json`.
 *
 * This file is the single source for the epoch strings, and a service builds
 * its cache keys from the same object it hands the writer. That is what makes
 * `tile.epoch.*` byte-identical to the cache key rather than merely intended
 * to be: there is only one string, so there is nothing to drift from.
 */
export interface EpochsFile {
  service: string;
  tilesets: Record<string, Epoch>;
}

export class UnknownTileset extends Error {
  constructor(
    readonly tileset: string,
    known: string[],
  ) {
    super(
      `okibi.epochs.json has no epochs for tileset ${JSON.stringify(tileset)}` +
        (known.length ? ` (it has ${known.join(", ")})` : " (it has none)"),
    );
    this.name = "UnknownTileset";
  }
}

export function epochFor(epochs: EpochsFile, tileset: string): Epoch {
  const epoch = epochs.tilesets[tileset];
  if (!epoch) throw new UnknownTileset(tileset, Object.keys(epochs.tilesets));
  return epoch;
}

/**
 * The cache key fragment for a tileset, from the same strings the events
 * carry.
 *
 * A service is free to build its keys some other way; what it is not free to
 * do is build them from a second copy of the epochs.
 */
export function cacheKeyFor(epochs: EpochsFile, tileset: string): string {
  const { source, algo, param } = epochFor(epochs, tileset);
  return `${source}/${algo}/${param}`;
}
