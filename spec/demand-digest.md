# The demand digest

Version `tile-demand-digest/1`.

Aggregated demand, and **the only demand data the planner reads**. Every
[binding](bindings/) has exactly one job beyond writing: produce this, in its
own dialect. Nothing downstream of here knows which backend the events came
from.

Schema: [`schema/demand-digest.schema.json`](schema/demand-digest.schema.json).
Example: [`examples/demand-digest.json`](examples/demand-digest.json).

## Format

JSONL or Parquet. One record per `(service, tileset, kind, qk8, window)`.

```jsonc
{ "digest": "tile-demand-digest/1",
  "service": "papers", "tileset": "style-aoi-04", "kind": "content",
  "qk8": "13300211",
  "window": "2026-08-23/P1D",
  "req": 48210,
  "miss": 312,
  "p50_gen_ms": 28900, "p95_gen_ms": 41200, "sum_gen_ms": 9016800,
  "avg_bytes": 88231, "bytes": 4.2e9,
  "tiles_observed": 1240,
  "sample_interval_max": 1,
  "top_qk": [ ["13300211231022", "14/14552/6451", 1820],
              ["13300211231023", "14/14553/6451", 1544] ] }
```

| Field | Meaning |
|---|---|
| `window` | An ISO 8601 interval |
| `req` | Total requests, sampling weight restored, **counting `organic` only** |
| `miss` | Of those, how many missed |
| `p50_gen_ms`, `p95_gen_ms` | Generation time **over misses only**. A hit generated nothing, and a cell that mostly hits would otherwise have a median of zero. May include `warm` requests |
| `sum_gen_ms` | Total generation time, over every request |
| `avg_bytes`, `bytes` | Response sizes |
| `tiles_observed` | Distinct tiles actually seen in this cell. The denominator every estimate is built on |
| `sample_interval_max` | How many events one row stood for, at worst. `1` means nothing here was sampled |
| `top_qk` | Optional. The cell's top tiles, `[qk, id, req]`, default top 20 |

`sample_interval_max` is recorded and not acted on. A backend that samples
keeps the totals right — the weight is restored when they are summed — but it
cannot keep the tail right: a cell whose rows each stood for a hundred events
knows roughly how much demand it had and very little about which tiles it was
spread across. When an estimate turns out wrong, this is the first thing worth
looking at, and it is only there to be looked at if it was written down while
the rows still existed.

A top tile carries both keys because both are needed and neither implies the
other. The quadkey says where the tile is, which is what the invalidation
scope is matched against and what makes an ancestor recognisable. The native
id is what the service's URL template is filled with, and okibi has no way to
derive one from the other — that is precisely the knowledge it does not have
about a service.

Records with `kind != content` set `qk8` to `"-"` and carry `top_id` instead
of `top_qk`, whose entries are `[id, req]`: a document with no coordinates has
no quadkey to carry. This is how a `tileset.json` — which has no
coordinates and cannot be placed in a cell — still ends up in a plan.

`top_qk` and `top_id` are what let a plan be finer than the digest. A digest
cell is eight quadkey characters wide, which is far coarser than a tile; the
top-N list carries a little of the resolution that the aggregation threw away,
so the planner can name individual tiles instead of guessing at the cell.

## Producing one

**Daily by default.**

**One implementation per binding, not per service.** Under the WAE binding
there is a single dataset and the index is `service`, so one process reads
every service in one query. A service list is configuration, not code.

**Append only.** A past window may be regenerated only as a re-derivation from
the same inputs — never edited to a different answer, because a plan derived
from it claims to be reproducible.
