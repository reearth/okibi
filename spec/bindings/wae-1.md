# Binding: Workers Analytics Engine

Version `wae-1`. Binds [`tile-demand/1`](../tile-demand.md) to Cloudflare
Workers Analytics Engine.

Everything Analytics Engine imposes lives in this page and nowhere else: 20
blobs, 20 doubles and one index per data point; 16 KB of blobs and 96 bytes of
index; three months of retention; sampling applied per index value.

## Dataset

`tile_demand_1`. The trailing number is the schema version.

Analytics Engine has no migrations, so a schema change is not an alter. It is
a new dataset, a period during which both are written, and then a reader
cutover. Append-only versioning, the same discipline engi applies to a file
that cannot be rewritten.

## Columns

| Attribute | Position |
|---|---|
| `tile.service` | `blob1` **and** `index1` |
| `tile.tileset` | `blob2` |
| `tile.kind` | `blob3` |
| `tile.id` | `blob4` |
| `tile.qk` | `blob5` |
| `tile.qk8` | `blob6` |
| `tile.cache.status` | `blob7` |
| `tile.epoch.source` | `blob8` |
| `tile.epoch.algo` | `blob9` |
| `tile.epoch.param` | `blob10` |
| `tile.fmt` | `blob11` |
| `tile.colo` | `blob12`, empty string when absent |
| `tile.origin` | `blob13` |
| `tile.cache.layer` | `blob14`, empty string when absent |
| `tile.site` | `blob15`, empty string when absent |
| `tile.count` | `double1` |
| `tile.gen_ms` | `double2` |
| `tile.gen_dep_ms` | `double3`, `0` when absent |
| `tile.bytes` | `double4` |
| `tile.z` | `double5` |

Five blobs and fifteen doubles are left over.

### Why the index is `service` alone

Sampling in Analytics Engine is applied per index value, which makes the index
the unit within which statistical accuracy is guaranteed. What okibi needs is
relative ordering *within* a service — is this cell hotter than that one — and
service granularity delivers exactly that.

It is also the fastest filter in a `WHERE`, and "read one service's demand" is
the access pattern every reader has.

## Reading

**Restore the sampling weight, always.** `SUM(double1 * _sample_interval)`. A
bare `count()` is not a smaller number than the truth; it is an arbitrary one,
because the interval varies with volume. Any frequency read that skips this is
invalid as a ledger and should not be treated as approximate.

There are two samplings and a row carries both: Analytics Engine's, in
`_sample_interval`, and the writer's, in `double1` when it
[samples hits](../tile-demand.md#sampling-hits). A row stands for
`double1 * _sample_interval` requests, so anything summed per request carries
both factors — `SUM(double4 * double1 * _sample_interval)` for bytes, not
`SUM(double4 * _sample_interval)`, which would count a kept hit's response once
for the ten it stood for. How coarsely a cell was read is likewise
`MAX(double1 * _sample_interval)`.

Generation time needs only the interval. Only hits are sampled, a hit
generated nothing, and misses are written one for one — so a quantile over
misses weighted by `_sample_interval` is already right, and stays right only
for as long as that rule holds.

**Aggregate space by `blob6`**, or by `blob5 LIKE '<prefix>%'` for a prefix
roll-up.

**Filter demand to `blob13 = 'organic'`.** Cost aggregates over `gen_ms` need
no such filter — see [the vocabulary](../tile-demand.md#rules-for-writing) for
why the two differ.

**Read the result as a lower bound.** An event is written by the service, so a
request answered before the service runs is a request nothing records. On
Cloudflare that is the edge cache in front of the Worker, and it is not a
rounding error: on 2026-08-26 the three services okibi was built for saw

| | client requests | answered at the edge | recorded |
|---|---|---|---|
| Terrain | 19,305,507 | 12,049,583 (62%) | 8,158,472 |
| Papers | 166,300 | 35,870 (22%) | 35,207 |
| Buildings | 296,011 | 1,396 (0.5%) | 7,023 |

The spread is the point. What the edge absorbs is what repeats within one
colo, which is the head of the distribution — so the loss is not uniform, it
is largest exactly where demand concentrates.

Two consequences, and it is worth being clear about which is which.

A digest therefore describes demand **flatter than it is**. Warming is ranked
by demand, and a loss that grows with popularity compresses the ranking rather
than reordering it, so a plan still warms the head first. What it gets wrong
is the size: `coverage_of_demand` reads low and the wait `no_warm` describes
reads short. okibi undersells itself, which is the safe direction for a number
somebody decides with.

That the ranking survives is an argument, not a measurement. A service whose
edge absorbs its head unevenly could have its order changed, and nothing here
would say so.

Bypassing the edge would fix the ledger and defeat the purpose: that cache is
one of the layers warming exists to fill. The bound is the honest answer.

## Writing

```ts
env.TILE_DEMAND.writeDataPoint({
  indexes: [ev.service],
  blobs: [ev.service, ev.tileset, ev.kind, ev.id, ev.qk, ev.qk8,
          ev.cacheStatus, ev.epochSource, ev.epochAlgo, ev.epochParam,
          ev.fmt, ev.colo ?? "", ev.origin, ev.cacheLayer ?? "",
          ev.site ?? ""],
  doubles: [ev.count ?? 1, ev.genMs, ev.genDepMs ?? 0, ev.bytes, ev.z ?? -1],
});
```

`double1` is `1` for every event unless the writer samples hits, in which case
a kept organic hit carries `k` and the others it stands for are not written at
all. Nothing else about the point changes.

Analytics Engine is what makes sampling worth having here. The dataset took
about 397 million data points in the month to 2026-09-29, about 99% of them
Terrain's. It is not billed yet; at the published $0.25 per million beyond ten
million, that is about $97 a month, and one hit in ten would take off about
86% of it. The precision it costs is in
[the vocabulary](../tile-demand.md#sampling-hits), and it is small beside
Analytics Engine's own: Terrain's edge hits were already being read at a
`_sample_interval` of up to 500.

Services should not write this themselves.
[`@reearth/okibi/writer`](../../packages/okibi) packs the columns, so
that the ordering above exists in one place rather than three.

## What adopting this binding obliges you to

Retention is three months, which is shorter than the history a demand model
wants. **Anyone using this binding must produce a [demand
digest](../demand-digest.md) daily and put it somewhere permanent.**

That single obligation settles two problems at once. The digests outlive the
retention window, and the planner never learns any SQL dialect — which is what
allows a second binding to exist at all.
