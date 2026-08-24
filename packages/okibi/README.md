# @reearth/okibi

The projection a service needs when it writes `tile.qk`: the
[okibi planner's own](../../crates/okibi-qk), compiled to wasm rather than
reimplemented in TypeScript.

There is no TypeScript in this directory and there is not meant to be. The
source is [`crates/okibi-wasm`](../../crates/okibi-wasm); what is committed
here is the packaging — the export map, the licences, and the tests that run
the built package the way a service will. `bundler/` and `nodejs/` appear when
you build, and are not committed, because a committed build product is one
that can be stale while looking authoritative.

The arithmetic is easy to write twice and hard to notice being subtly wrong.
A tile projected into the wrong cell is invisible — the events keep arriving,
the digest keeps aggregating — until a plan warms the wrong part of the world.

```ts
import { qk8, quadkeyForTile } from "@reearth/okibi";

// Terrain: geographic, two root tiles wide, y from the south.
const qk = quadkeyForTile("geographic-tms", 14, 29108, 11439);
qk8(qk); // "13300211" — the cell a demand digest aggregates into
```

Schemes: `web-mercator`, `web-mercator-tms`, `geographic`, `geographic-tms`.
A tile's centre point is what crosses between them, because the same
coordinates mean different ground in each.

Also here: `quadkeyForTileAt` for a level other than the tile's own,
`quadkeyForPoint`, and `startsWith` for matching an invalidation scope.

## The digest

The same aggregation `okibi digest` runs, for a service that would rather take
its own digest from a Worker cron than from a scheduled CI job:

```ts
import { assembleDigest, digestQueries } from "@reearth/okibi";

const { cells, topTiles } = digestQueries({ services: ["papers"] }, "2026-08-23");
const { records, skipped } = assembleDigest(cellRows, tileRows, "2026-08-23", 20);
```

It is here rather than rewritten in TypeScript for the same reason the
projection is. Which cell an unplaced request belongs to, how a tie between
two equally hot tiles breaks, what happens to a row that cannot be placed —
none of those fail loudly when two implementations disagree, and a digest that
means something slightly different is a plan that warms somewhere slightly
wrong.

Two rules of the [binding](../../spec/bindings/wae-1.md) live in the query text
for the same reason: every frequency carries `_sample_interval`, and demand
counts organic requests only.
[`examples/okibi-digest-cron.ts`](../../examples/okibi-digest-cron.ts) is the
rest of what a Worker needs, which is an HTTP call and a bucket write.

Planning is not in this package yet. When it is, it arrives as more exports
from the same place.

## Building

`pkg` is built, not committed:

```sh
pnpm build   # or scripts/build-wasm.sh from the repository root
```

Two targets ship. `bundler` is what wrangler bundles into a Worker; `nodejs`
is what a test can import directly. The export map picks between them.
