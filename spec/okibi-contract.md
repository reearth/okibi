# The okibi contract

The documents around the planner. Three go in, one comes out, and a fourth —
the pricing table — turns the plan's resource counts into money.

| | | |
|---|---|---|
| [Service manifest](#service-manifest) | `okibi-service/1` | what a service says about itself |
| [Invalidation event](#invalidation-event) | `okibi-invalidation/1` | what died |
| [Pricing table](#pricing-table) | `okibi-pricing/1` | what a unit of resource costs |
| [Warm plan](#warm-plan) | `okibi-warm-plan/1` | what to fetch, in what order, at what cost |

Schemas are under [`schema/`](schema/) and one valid example of each is under
[`examples/`](examples/).

## Service manifest

Everything okibi is allowed to know about a service. There is no other channel:
if it is not in here or in a digest, the planner cannot act on it.

```jsonc
{ "manifest": "okibi-service/1",
  "service": "papers",
  "url_template": "https://papers.reearth.land/t/{tileset}/{id}?e={epoch.param}",
  "meta_urls": { "tileset": "https://papers.reearth.land/t/{tileset}/meta.json" },
  "cost": {
    "default_gen_ms": 30000,
    "default_bytes": 90000,
    "concurrency_limit": 4,
    "rate_per_s": 2,
    "billing": {
      "pricing_profile": "cloudflare",
      "per_gen": {
        "cpu_ms": 1800,
        "container_memory_gb_s": 15,
        "container_vcpu_s": 30,
        "subrequest": 3,
        "storage_class_a": 1,
        "egress_byte": null
      }
    }
  },
  "lanes": { "interactive_priority": true },
  "depends_on": [
    { "service": "terrain", "reason": "elevation lookup", "order": "before" }
  ],
  "zoom_semantics": "resolution" }
```

`url_template` is what makes warming possible without okibi understanding the
service: substitute `{tileset}`, `{id}` and the epochs, and the result is a URL
that regenerates the tile. The on-demand path is the generator.

`default_gen_ms` and `default_bytes` are fallbacks for cells with no
measurement. Anything with observed demand has real numbers in the digest.

`concurrency_limit` and `rate_per_s` bound what the origin will tolerate. They
are also what turns a plan into a duration, which is what decides whether it
fits in a CI job.

`billing` counts resources per generation and holds **no prices**. Prices move;
a manifest that carried them would silently change the meaning of old
estimates.

`per_gen` is keyed the way the pricing table keys its units, so a cost is the
two multiplied together with nothing mapping between them. It is also why a
service can bill for something this specification has never heard of — a
container's memory-seconds, a GPU — without anyone revising a schema: the
resource is named in both files, or it is priced at nothing.

A `null` amount says to measure it rather than to assume it. Two resources
have a measurement that means them: `cpu_ms` falls back to the digest's
`p50_gen_ms` and `egress_byte` to its `avg_bytes`. Anything else left null
counts as nothing, because inventing a number for a resource nobody measured
would put it in the estimate as though it were known.

`depends_on` with `order: "before"` puts the dependency's tiles for the same
space ahead of the dependent's: a service warms after whatever it will call
while generating, so that those calls are hits rather than the generation it
was about to pay for.

`zoom_semantics` is `"resolution"` or `"size_bucket"`, and it decides whether
the ancestor optimisation applies. Under `size_bucket` a shallower zoom is not
a coarser view of the same ground and warming it saves nobody anything.

## Invalidation event

The normalised form of "something died". Whatever mechanism a service uses to
invalidate, it hands okibi one of these.

```jsonc
{ "event": "okibi-invalidation/1",
  "service": "papers", "tileset": "style-aoi-04",
  "axis": "param",
  "epoch_from": "style-aoi-04@r12",
  "epoch_to":   "style-aoi-04@r13",
  "scope": { "type": "qk_prefixes", "prefixes": ["133002"] },
  "occurred_at": "2026-08-24T02:00:00Z",
  "deadline": "2026-08-24T08:00:00Z" }
```

`axis` is `source`, `algo` or `param`.

`scope.type` is `all`, `qk_prefixes` or `ids`.

`deadline` is optional, and is a time the warming should be finished by —
the start of a working day, typically. It is an input, not a wall clock the
planner reads.

A service does not normally build one of these by hand. `okibi.epochs.json` in
the service repository is the single source for the epoch strings, and the
plan action derives the event from that file's git diff — so the event is a
consequence of the commit rather than a second description of it.

## Pricing table

Unit prices for one profile, in one month. Lives in
[`pricing/`](../pricing/) in this repository.

```jsonc
{ "pricing": "okibi-pricing/1",
  "profile": "cloudflare",
  "effective": "2026-08",
  "currency": "USD",
  "units": {
    "cpu_ms": 0.0000000125,
    "container_memory_gb_s": 0.0000025,
    "container_vcpu_s": 0.00002,
    "subrequest": 0.0000004,
    "storage_class_a": 0.0000045,
    "egress_byte": 0.0 } }
```

Each key under `units` is a price per one of the resources a manifest's
`per_gen` counts, and the two are the same keys, so they multiply directly.
A resource the table does not price is priced at nothing — right for R2's
egress, and the reason an estimate is only ever as complete as its table.

**Pricing files are append-only.** A price change is a new file for a new
month; editing an old one would make the estimates that cite it unreproducible,
and a plan records the hash of the table it used precisely so that it stays
checkable years later.

## Warm plan

The planner's output. It is JSON, and that is the point: it can be stored,
diffed, reviewed and re-derived.

```jsonc
{ "plan": "okibi-warm-plan/1",
  "derived_from": {
    "digest": ["r2://okibi/digests/2026-08-23.jsonl"],
    "invalidation": "sha256:…",
    "manifests": { "papers": "sha256:…", "terrain": "sha256:…" }
  },
  "entries": [
    { "url": "https://papers.reearth.land/t/style-aoi-04/14/14552/6451?e=style-aoi-04@r13",
      "service": "papers", "priority": 0.982,
      "lane": "warm",
      "expected_gen_ms": 34120, "saved_req_estimate": 1820 }
  ],
  "stats": { "total": 4210, "sum_expected_gen_ms": 9.1e7, "coverage_of_demand": 0.93 },
  "estimate": {
    "pricing": "pricing/cloudflare-2026-08.json@sha256:…",
    "warm":    { "tiles": 4210, "wall_clock_s": 8830, "cpu_ms": 1.18e8,
                 "usd": 6.42, "storage_delta_bytes": 4.2e8 },
    "no_warm": { "affected_first_requests": 7900, "user_wait_ms_total": 2.4e8,
                 "p95_first_byte_ms": 34000 },
    "reclaimable": { "prev_epoch_bytes": 3.9e8 },
    "marginal": [ { "coverage": 0.50, "tiles":  520, "usd": 1.10, "wall_clock_s":  1400 },
                  { "coverage": 0.80, "tiles": 1680, "usd": 3.20, "wall_clock_s":  4100 },
                  { "coverage": 0.90, "tiles": 3050, "usd": 5.10, "wall_clock_s":  7000 },
                  { "coverage": 0.95, "tiles": 7400, "usd": 9.80, "wall_clock_s": 16800 } ]
  } }
```

`derived_from` records which inputs produced this, by hash. With it, a plan is
a derived artifact rather than a claim: the originals are the digest, the
event and the manifests, and the same three give back the same plan whenever
anyone asks. See [the planner](planner.md#determinism) for what makes that
true rather than merely intended.

`entries` are ordered. The executor takes them from the top and needs to
understand nothing else — `lane`, the optional `not_before`, and the rate
limits are all it reads. Anything that can issue GETs in order is a conforming executor.

`estimate` is computed with the plan and not by a second pass, which is why
there is no separate "how much would this cost" mode anywhere in okibi. What
it costs is a property of what was decided. Its four numbers are described in
[the planner](planner.md#the-estimate).
