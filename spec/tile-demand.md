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
| `tile.service` | string | ✔ | Which service. The set in use is the operator's; what this requires is that the same service is the same string in every event, digest and plan |
| `tile.tileset` | string | ✔ | Which tileset within that service, e.g. `cesium-mesh/ellipsoid`, `plateau-tokyo23` |
| `tile.kind` | string | ✔ | `content`, `tileset`, `subtree` or `meta`. Anything with no tile coordinates — `tileset.json`, `layer.json`, a subtree file — is not `content` |
| `tile.id` | string | ✔ | The **native** tile identifier. Combined with the manifest's URL template it must reconstruct the original URL exactly. For `kind != content`, the request path or something equivalent to it |
| `tile.qk` | string | when `content` | The **normalised spatial key**: the tile's centre point projected to a Web Mercator quadkey. May be empty when `kind != content` |
| `tile.qk8` | string | when `content` | `tile.qk` truncated to 8 characters, or the whole thing if it is shorter. For aggregation only |
| `tile.cache.status` | string | ✔ | `hit`, `miss`, `swr` or `error` |
| `tile.cache.layer` | string | — | Which layer answered a hit: `client`, `edge`, `store`, or a name the service registers. Absent on a miss |
| `tile.epoch.source` | string | one of | The source-data part of the cache key, **byte-identical to what is in it** |
| `tile.epoch.algo` | string | the three | The algorithm part. Likewise |
| `tile.epoch.param` | string | required | The parameter part. Likewise |
| `tile.fmt` | string | ✔ | Delivered format: `qmesh`, `png`, `mvt`, `glb`, `json`, … |
| `tile.colo` | string | — | Edge location code, e.g. `NRT`, where one is available |
| `tile.origin` | string | ✔ | `organic` or `warm`. A request okibi itself made is `warm` |
| `tile.count` | number | ✔ | Always `1`. It exists so the reader can restore sampling weight |
| `tile.gen_ms` | number | ✔ | Milliseconds spent generating, as far as the runtime can see. `0` on a hit |
| `tile.gen_dep_ms` | number | — | The part of `gen_ms` spent calling another service, e.g. a building-mesh service asking a terrain service for ground height |
| `tile.bytes` | number | ✔ | Response body size in bytes |
| `tile.z` | number | when `content` | The native zoom level. **Only comparable within one service** |

Two of these carry a caveat worth stating twice.

`tile.qk` is what makes services comparable at all. One may tile a geographic
grid, another Web Mercator, a third a 3D Tiles subdivision; the same
coordinates mean different ground in each, so they cannot be aggregated
together. A centre point projected into one quadkey space can. Projecting is the service's job,
though it need not write the projection itself — [`okibi-qk`](../crates/okibi-qk)
exists to be used here.

`tile.cache.layer` is not what decides whether a tile is worth warming — a hit
is somebody wanting the tile whichever layer had it — but it is what decides
what serving one costs. An edge hit costs nothing; a hit from an object store
is a read operation with a price on it. Without the distinction, the bill for
serving is a range rather than a number, and okibi's whole disposition is that
a cost you cannot see is a cost nobody decides about.

Three names are registered. `client` is a revalidation answered `304` — the
requester already had the bytes and only asked whether they were still
current, so nothing was read anywhere. `edge` is a cache in front of the
service. `store` is an object store, and is the one that costs a read.

A service with a layer that is none of those names it, and a reader that does
not recognise a name should count it as neither free nor priced rather than
guess.

`tile.gen_ms` is a lower bound, not a stopwatch. A runtime may freeze its
clocks between I/O — Workers do, to blunt Spectre — so a generator that is
pure CPU measures as zero however long it ran, and one that mixes CPU with
fetches measures only the fetches. Time the whole of generating, including
the calls it makes, and read the clock after an I/O rather than before one.

A service whose cost this cannot reach should say so in its manifest instead:
`cost.billing.per_gen.cpu_ms` is a number a service knows about itself, and
where it is given, an estimate stops depending on a measurement the runtime
will not take. A planner reading a zero treats it as no measurement at all,
because a tile that took no time to make does not exist.

`tile.z` is *not* comparable across services, and no aggregate should treat it
as though it were. A service may use zoom as a size bucket, in which case its
`z=13` is a statement about how much geometry is in a tile rather than about
how large the tile is on the ground. This is why the planner's ancestor optimisation is opt-in
per service rather than universal — see [the planner](planner.md).

## Rules for writing

**Write unconditionally.** Every tile request gets an event, hits included. A
ledger of misses would record where the cache failed; what okibi needs is
where the demand is, and demand is mostly hits.

**Write on the hot path.** Emit as the response goes out. Where the backend
offers a non-blocking write, do not wait for it.

**An epoch must agree with what an invalidation says.** That is the whole of
the requirement, and the only thing an epoch is read for: asking, after a
change, how much demand there was under the thing that just died. The answer
is a join between the ledger and the epochs the service reports, and a join
only works when both sides spell it the same way.

The cache key is how that is usually achieved rather than what is asked for.
Build the key and the event from one construction and the two cannot disagree;
build them separately and they agree until the day they do not.

**Split what moves separately.** The three names say what a part is *for* —
where the data came from, how it was built, what it was built with — and okibi
decides nothing by them. What they are worth is later, in a query nobody has
written yet: two versions folded into one string can never be asked about
apart, and a service that bumps a renderer without touching a style has lost
the ability to ask what that cost. Give a thing its own axis when it can move
on its own.

A service with fewer than three such things names fewer and leaves the rest
empty. What it must not do is derive an axis a second way to fill a name — an
epoch computed twice is two strings that agree until they do not. If they drift, okibi can no longer match "an
invalidation happened" against "these tiles are the ones that died", and it
will confidently warm the wrong set. Do not maintain this by discipline:
read both the cache key and the event from one file, `okibi.epochs.json`, so
that drift is not expressible.

Where a service resolves part of its key at request time — the current
upstream snapshot, a pointer it follows — no file can hold that epoch, and the
event carries what the key was actually built from. The requirement is the
key, not the file; the file is how the rest of it is kept honest. Such a
service reports its resolved epochs at `/okibi/epochs.json`, so that a change
nobody deployed is still something okibi can see.

**Mark warmed requests, and only okibi's.** The executor sends
`X-Okibi-Warm: <shared secret>`; a service writes `tile.origin: "warm"` only
when the value matches the secret it was configured with.

Counting okibi's own requests as demand would make whatever okibi warmed look
more popular next time, which would make okibi warm it again — a loop that
ends with the plan describing the planner's history instead of anyone's
traffic. So the mark has to exist.

It has to be unforgeable for the opposite reason. A bare `X-Okibi-Warm: 1`
that any client could send is a way for anyone on the internet to remove their
own requests from the ledger — and demand that is not recorded is demand that
is never warmed. The ledger is meant to be a record of what people asked for,
not a record of what people were willing to admit to.

A service with no secret configured marks nothing as warm. That errs toward
counting okibi's own traffic as demand, which is a bounded and visible error;
the other direction is a ledger anyone can edit.

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
  "tile.kind": "content", "tile.id": "14/29108/11439",
  "tile.qk": "13300211231032", "tile.qk8": "13300211",
  "tile.cache.status": "miss",
  "tile.epoch.source": "gsi-dem-2026a", "tile.epoch.algo": "tc-0.9.2",
  "tile.epoch.param": "me1.0-wm",
  "tile.fmt": "qmesh", "tile.colo": "NRT", "tile.origin": "organic",
  "tile.count": 1, "tile.gen_ms": 2380, "tile.bytes": 41200, "tile.z": 14 }
```

The other examples are a Buildings tile with its dependent-call time broken
out, a Buildings `tileset.json` request that carries no coordinates at all and
was answered by an edge cache, and a Papers tile that okibi warmed itself.
