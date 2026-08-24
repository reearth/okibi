# @reearth/okibi-writer

One [tile-demand event](../../spec/tile-demand.md) per tile request, written
from inside a service.

Adopting the vocabulary should cost a service one call. Two of the things it
would otherwise have to get right alone fail silently, which is the worst way
to fail — nothing errors, events keep arriving, and the ledger is quietly
describing something else.

```ts
import { createWriter, originOf } from "@reearth/okibi-writer";
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
  origin: originOf(request),
  genMs: 34120,
  bytes: 88231,
  z: 14,
});
```

**The epochs come from the file the cache keys come from.** `tile.epoch.*` has
to be byte-identical to the strings in the cache key; the only way to hold
that is for there to be one string rather than two that agree today.
`cacheKeyFor(epochs, tileset)` is here for the other half of that.

**The column order lives in one place.** Three services packing thirteen blobs
by hand is three chances to shift one by a position and produce a ledger that
looks fine and means something different.

**`write` never throws.** It runs as the response goes out, and no tile
response is worth losing over bookkeeping. A refused event goes to `onError`
instead. The pure `toDataPoint` does throw, which is what tests can hold.

Projection is not here: `tile.qk` comes from
[`@reearth/okibi`](../okibi), which is the planner's own.
