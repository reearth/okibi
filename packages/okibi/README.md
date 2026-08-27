# @reearth/okibi

Everything a service needs from [okibi](../..): the one call it makes per tile
request, the projection that call needs, and the aggregation it runs if it
takes its own digest.

There is no TypeScript in `bundler/` or `nodejs/` and there is not meant to
be. They are built from [`crates/okibi-wasm`](../../crates/okibi-wasm) and are
not committed, because a committed build product is one that can be stale
while looking authoritative. `writer/` is ordinary TypeScript and is.

## Writing events

```ts
import { createWriter, originOf } from "@reearth/okibi/writer";
import { quadkeyForTile } from "@reearth/okibi";

import epochs from "../okibi.epochs.json";

const writer = createWriter({ dataset: env.TILE_DEMAND, epochs });

writer.write({
  tileset: "style-aoi-04",
  kind: "content",
  id: "14/14552/6451",
  qk: quadkeyForTile("web-mercator", 14, 14552, 6451),
  cacheStatus: "miss",
  fmt: "png",
  origin: originOf(request, env.OKIBI_WARM_SECRET),
  genMs: 34120,
  bytes: 88231,
  z: 14,
});
```

`/writer` is a separate entry point because it is plain TypeScript with no
dependencies: a service that gets `tile.qk` some other way imports it without
pulling in the wasm.

**The epochs come from the file the cache keys come from.** `tile.epoch.*` has
to be byte-identical to the strings in the cache key, and the only way to hold
that is for there to be one string rather than two that agree today.
`cacheKeyFor(epochs, tileset)` is here for the other half of it.

**The column order lives in one place.** Three services packing thirteen blobs
by hand is three chances to shift one by a position and produce a ledger that
looks fine and means something different.

**`write` never throws.** It runs as the response goes out, and no tile
response is worth losing over bookkeeping; a refused event goes to `onError`.
The pure `toDataPoint` does throw, which is what tests hold.

## Projection

```ts
import { qk8, quadkeyForTile } from "@reearth/okibi";

// Terrain: geographic, two root tiles wide, y from the south.
const qk = quadkeyForTile("geographic-tms", 14, 29108, 11439);
qk8(qk); // "13300211" — the cell a demand digest aggregates into
```

Schemes: `web-mercator`, `web-mercator-tms`, `geographic`, `geographic-tms`.
A tile's centre point is what crosses between them, because the same
coordinates mean different ground in each. Also here: `quadkeyForTileAt` for a
level other than the tile's own, `quadkeyForPoint`, and `startsWith` for
matching an invalidation scope.

This is [the planner's own projection](../../crates/okibi-qk) rather than a
second implementation. The arithmetic is easy to write twice and hard to
notice being subtly wrong: a tile projected into the wrong cell is invisible —
the events keep arriving, the digest keeps aggregating — until a plan warms
the wrong part of the world.

## Taking a digest

```ts
import { assembleDigest, digestQueries } from "@reearth/okibi";

const { cells, topTiles } = digestQueries({ services: ["papers"] }, "2026-08-23");
const { records, skipped } = assembleDigest(cellRows, tileRows, "2026-08-23", 20);
```

`topTiles` is for one service. Its row limit is a row count, and one query
ordered by demand spends all of it on whichever service is busiest — leaving
the slow services, the ones warming is for, with nothing to plan from. Name
the service in the config as above, pass it as a third argument, or take
`topTiles` as `null` and ask once per service the cells query turned up.

The same aggregation `okibi digest` runs, for a service that would rather take
its own from a Worker cron than from a scheduled CI job. Here for the same
reason the projection is: which cell an unplaced request belongs to, how a tie
between two equally hot tiles breaks, what happens to a row that cannot be
placed — none of those fail loudly when two implementations disagree.

Two rules of the [binding](../../spec/bindings/wae-1.md) live in the query text
for the same reason: every frequency carries `_sample_interval`, and demand
counts organic requests only.
[`examples/okibi-digest-cron.ts`](../../examples/okibi-digest-cron.ts) is the
rest of what a Worker needs, which is an HTTP call and a bucket write.

Planning is not here yet. When it is, it arrives as more exports from the same
place.

## In a Worker

A Worker importing both entry points bundles to about 78 KiB gzipped, wasm
included. `wrangler deploy --dry-run` over a Worker that calls `createWriter`
and `quadkeyForTile` is what that number is measured from, and is worth
running once against a service before believing the build works there.

## Building

```sh
pnpm build   # wasm-pack for both targets, then tsc for the writer
```

Two wasm targets ship: `bundler` is what wrangler puts in a Worker, `nodejs`
is what a test can import directly. The export map picks between them.
