/** The `tile-demand/1` vocabulary, as TypeScript. See `spec/tile-demand.md`. */

export type TileKind = "content" | "tileset" | "subtree" | "meta";

export type CacheStatus = "hit" | "miss" | "swr" | "error";

/**
 * Where a request came from. `warm` is okibi asking for a tile itself, and is
 * excluded from demand so that warming does not make its own choices look
 * popular next time.
 */
export type Origin = "organic" | "warm";

/**
 * The parts of a tile's cache key that are not per-tile, spelled the way the
 * key spells them.
 *
 * Three names for what a part is *for* — where the data came from, how it was
 * built, what it was built with — rather than three parts every service must
 * have. A key made of two pieces fills two and leaves the third empty.
 * Splitting one piece three ways to fill the names would put strings in an
 * event that appear in no cache key, which is the one thing these may not be.
 */
export interface Epoch {
  source?: string;
  algo?: string;
  param?: string;
}

/** One tile request. */
export interface TileDemandEvent {
  service: string;
  tileset: string;
  kind: TileKind;
  /** The native tile id, which with the URL template rebuilds the request. */
  id: string;
  /**
   * The tile's centre as a Web Mercator quadkey. Required for `content`.
   *
   * Services get this from `@reearth/okibi`, which is the same projection the
   * planner uses; the arithmetic is easy to reimplement and hard to notice
   * being subtly wrong.
   */
  qk?: string;
  cacheStatus: CacheStatus;
  epoch: Epoch;
  fmt: string;
  colo?: string;
  origin: Origin;
  /** Milliseconds spent generating. Zero on a hit. */
  genMs: number;
  /** The part of `genMs` that was spent calling another service. */
  genDepMs?: number;
  bytes: number;
  /** The native zoom. Required for `content`, and comparable only within one service. */
  z?: number;
}

/** What a service passes in: everything the writer cannot know for it. */
export type TileDemand = Omit<TileDemandEvent, "service" | "epoch"> & {
  /**
   * The epochs this tile was cached under, where they are not the ones in
   * `okibi.epochs.json`.
   *
   * Most of a cache key is decided when a service is built, and reading it
   * from the file the key is built from is what keeps the two identical. Some
   * of it is not: a service that resolves the current upstream snapshot at
   * request time has an epoch that no file could hold, and the requirement is
   * that the event match the key — not that it match a file.
   *
   * So the caller may hand over the strings it just built the key from. What
   * it must not do is build them a second way.
   */
  epoch?: Epoch;
};
