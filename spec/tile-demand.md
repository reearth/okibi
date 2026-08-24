# The tile-demand vocabulary

Version `tile-demand/1`.

One event per tile request, written by the service that served it. This page
defines what the attributes mean. It does not define where they are stored —
that is a [binding](bindings/), and there can be several.

Schema: [`schema/tile-demand-event.schema.json`](schema/tile-demand-event.schema.json).
Examples: [`examples/`](examples/).

## The attributes

| Attribute | Type | Required | Meaning |
|---|---|---|---|
| `tile.service` | string | ✔ | Which service. `terrain`, `buildings`, `papers`. A new one is registered by revising this page |
| `tile.tileset` | string | ✔ | Which tileset within that service, e.g. `cesium-mesh/ellipsoid`, `plateau-tokyo23` |
| `tile.kind` | string | ✔ | `content`, `tileset`, `subtree` or `meta`. Anything with no tile coordinates — `tileset.json`, `layer.json`, a subtree file — is not `content` |
| `tile.id` | string | ✔ | The **native** tile identifier. Combined with the manifest's URL template it must reconstruct the original URL exactly. For `kind != content`, the request path or something equivalent to it |
| `tile.qk` | string | when `content` | The **normalised spatial key**: the tile's centre point projected to a Web Mercator quadkey. May be empty when `kind != content` |
| `tile.qk8` | string | when `content` | `tile.qk` truncated to 8 characters, or the whole thing if it is shorter. For aggregation only |
| `tile.cache.status` | string | ✔ | `hit`, `miss`, `swr` or `error` |
| `tile.epoch.source` | string | ✔ | The source-data epoch. **Byte-identical to the string in the cache key** |
| `tile.epoch.algo` | string | ✔ | The algorithm epoch. Likewise |
| `tile.epoch.param` | string | ✔ | The parameter epoch. Likewise |
| `tile.fmt` | string | ✔ | Delivered format: `qmesh`, `png`, `mvt`, `glb`, `json`, … |
| `tile.colo` | string | — | Edge location code, e.g. `NRT`, where one is available |
| `tile.origin` | string | ✔ | `organic` or `warm`. A request okibi itself made is `warm` |
| `tile.count` | number | ✔ | Always `1`. It exists so the reader can restore sampling weight |
| `tile.gen_ms` | number | ✔ | Milliseconds spent generating. `0` on a hit |
| `tile.gen_dep_ms` | number | — | The part of `gen_ms` spent calling another service, e.g. Buildings asking Terrain for elevation |
| `tile.bytes` | number | ✔ | Response body size in bytes |
| `tile.z` | number | when `content` | The native zoom level. **Only comparable within one service** |

Two of these carry a caveat worth stating twice.

`tile.qk` is what makes the services comparable at all. Terrain is
TMS-Geographic, Buildings is 3D Tiles, Papers is Web Mercator; their tile
coordinates mean different things and cannot be aggregated together. A centre
point projected into one quadkey space can. Projecting is the service's job,
though it need not write the projection itself — [`okibi-qk`](../crates/okibi-qk)
exists to be used here.

`tile.z` is *not* comparable across services, and no aggregate should treat it
as though it were. Buildings uses zoom as a size bucket, so its `z=13` is a
statement about how much geometry is in a tile, not about how large the tile
is on the ground. This is why the planner's ancestor optimisation is opt-in
per service rather than universal — see [the planner](planner.md).

## Rules for writing

**Write unconditionally.** Every tile request gets an event, hits included. A
ledger of misses would record where the cache failed; what okibi needs is
where the demand is, and demand is mostly hits.

**Write on the hot path.** Emit as the response goes out. Where the backend
offers a non-blocking write, do not wait for it.

**Keep the epochs byte-identical.** `tile.epoch.*` must equal the strings the
cache key is built from, exactly. If they drift, okibi can no longer match "an
invalidation happened" against "these tiles are the ones that died", and it
will confidently warm the wrong set. Do not maintain this by discipline:
read both the cache key and the event from one file, `okibi.epochs.json`, so
that drift is not expressible.

**Mark warmed requests.** The executor sends `X-Okibi-Warm: 1`; a service that
sees it writes `tile.origin: "warm"`. Counting okibi's own requests as demand
would make whatever okibi warmed look more popular next time, which would make
okibi warm it again — a loop that ends with the plan describing the planner's
history instead of anyone's traffic. Readers exclude `warm` from demand for
this reason.

`gen_ms` is exempt: generation cost does not depend on who asked, so warm
requests are perfectly good cost samples and estimates may use them.

## Relationship to OpenTelemetry

A tile-demand event is an event — a structured log record — and not a trace or
a metric.

Not a trace, because tracing is meant to be sampled toward the interesting
requests, and a demand ledger has to be statistically complete or uniformly
sampled to be worth anything.

Not a metric, because `tile.qk` has effectively unbounded cardinality and will
not go in a label.

The attribute names follow the semantic-convention style, and are shaped to sit
in an OTLP LogRecord unchanged, so that an environment with a collector can
route them through it. That is an available transport, not a dependency: no
part of okibi requires OpenTelemetry to be present.

## Examples

A Terrain miss, quantized mesh, TMS-Geographic at z14
([`examples/tile-demand-event.terrain.json`](examples/tile-demand-event.terrain.json)):

```jsonc
{ "tile.service": "terrain", "tile.tileset": "cesium-mesh/ellipsoid",
  "tile.kind": "content", "tile.id": "14/29105/12903",
  "tile.qk": "13300211231021", "tile.qk8": "13300211",
  "tile.cache.status": "miss",
  "tile.epoch.source": "gsi-dem-2026a", "tile.epoch.algo": "tc-0.9.2",
  "tile.epoch.param": "me1.0-wm",
  "tile.fmt": "qmesh", "tile.colo": "NRT", "tile.origin": "organic",
  "tile.count": 1, "tile.gen_ms": 2380, "tile.bytes": 41200, "tile.z": 14 }
```

The other examples are a Buildings tile with its dependent-call time broken
out, a Buildings `tileset.json` request carrying no coordinates at all, and a
Papers tile that okibi warmed itself.
