// Writes one tile-demand event per tile request, from inside a service.
//
// What a service is left to do is describe the request it just served. Two
// things it would otherwise have to get right on its own are handled here,
// because both fail silently: the epochs come from the same file its cache
// keys do, and the Analytics Engine column order lives in one place instead of
// once per service.
//
// Projection is not here. `tile.qk` comes from `@reearth/okibi`, which is the
// planner's own projection compiled to wasm — see `spec/tile-demand.md`.

export type {
  CacheLayer,
  CacheStatus,
  Epoch,
  Origin,
  TileDemand,
  TileDemandEvent,
  TileKind,
} from "./types.js";

export { type EpochsFile, UnknownTileset, cacheKeyFor, epochFor } from "./epochs.js";
export { type HasHeaders, WARM_HEADER, originOf, warmHeaders } from "./origin.js";
export { type DataPoint, type Dataset, NotWritable, check, qk8, toDataPoint } from "./wae.js";
export { type Writer, type WriterOptions, createWriter } from "./writer.js";
