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
| `tile.count` | `double1` |
| `tile.gen_ms` | `double2` |
| `tile.gen_dep_ms` | `double3`, `0` when absent |
| `tile.bytes` | `double4` |
| `tile.z` | `double5` |

Seven blobs and fifteen doubles are left over.

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

**Aggregate space by `blob6`**, or by `blob5 LIKE '<prefix>%'` for a prefix
roll-up.

**Filter demand to `blob13 = 'organic'`.** Cost aggregates over `gen_ms` need
no such filter — see [the vocabulary](../tile-demand.md#rules-for-writing) for
why the two differ.

## Writing

```ts
env.TILE_DEMAND.writeDataPoint({
  indexes: [ev.service],
  blobs: [ev.service, ev.tileset, ev.kind, ev.id, ev.qk, ev.qk8,
          ev.cacheStatus, ev.epochSource, ev.epochAlgo, ev.epochParam,
          ev.fmt, ev.colo ?? "", ev.origin],
  doubles: [1, ev.genMs, ev.genDepMs ?? 0, ev.bytes, ev.z ?? -1],
});
```

Services should not write this themselves.
[`@reearth/okibi-writer`](../../packages/okibi-writer) packs the columns, so
that the ordering above exists in one place rather than three.

## What adopting this binding obliges you to

Retention is three months, which is shorter than the history a demand model
wants. **Anyone using this binding must produce a [demand
digest](../demand-digest.md) daily and put it somewhere permanent.**

That single obligation settles two problems at once. The digests outlive the
retention window, and the planner never learns any SQL dialect — which is what
allows a second binding to exist at all.
