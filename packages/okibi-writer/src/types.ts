/** The `tile-demand/1` vocabulary, as TypeScript. See `spec/tile-demand.md`. */

export type TileKind = "content" | "tileset" | "subtree" | "meta";

export type CacheStatus = "hit" | "miss" | "swr" | "error";

/**
 * Where a request came from. `warm` is okibi asking for a tile itself, and is
 * excluded from demand so that warming does not make its own choices look
 * popular next time.
 */
export type Origin = "organic" | "warm";

/** The three axes a tile's cache key is built from. */
export interface Epoch {
  source: string;
  algo: string;
  param: string;
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
export type TileDemand = Omit<TileDemandEvent, "service" | "epoch">;
