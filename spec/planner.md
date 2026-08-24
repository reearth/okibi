# The planner

```
plan :: (demand_digest[], invalidation_event, service_manifest[]) -> warm_plan
```

A pure function. No network, no clock, no randomness — the deadline arrives as
a field on the invalidation event, not from the machine the planner runs on.

The documents it reads and writes are in [the contract](okibi-contract.md).
This page is what happens in between, in enough detail to reimplement.

## Priority

Over the intersection of what the invalidation killed and what the digest says
anyone wants:

```
priority(cell) = normalise( freq(cell) × cost(cell) )

freq(cell) = weighted request count, several windows combined
             by exponential decay, half-life 7 days by default
cost(cell) = p50_gen_ms from the digest,
             falling back to the manifest's default_gen_ms
```

Only the intersection is considered. Demand outside the invalidation's scope
is still warm and needs nothing; invalidated space with no observed demand has
nobody waiting on it.

The cost factor is what lets one formula serve services with generation times
three orders of magnitude apart. A service that takes tens of seconds a tile
pays for itself on cells nobody would call hot; one that takes two and a half
seconds does not. Nothing selects a threshold per service — the threshold
moves on its own.

## Expansion and ordering

**Metadata first.** Entries with `kind != content` — `tileset.json`,
`layer.json` and friends — go at the head of the plan unconditionally. A cold
root document is not one slow tile; it is every client's first paint, for
every client, before any tile is even requested.

**Ancestors, where zoom means resolution.** For a service declaring
`zoom_semantics: "resolution"`, the ancestors of a selected tile that fall in
the same invalidation scope are added, shallower zooms first. An ancestor
rescues more requests per tile than any of its descendants.

Under `size_bucket` this is skipped entirely and only measured frequency
counts. A shallower bucket is a differently-sized tile of the same ground, so
warming it saves nothing that the deeper one would have.

**Cells become tiles.** Where `top_qk` exists, the cell's priority expands
onto the named tiles. Where it does not, the entries are the tiles the digest
actually observed in that cell — not a sweep of the cell down to some declared
maximum zoom. Depth nobody has ever requested is the long tail, and the long
tail is what on-demand generation is for.

**Dependencies before dependents.** A `depends_on` with `order: "before"`
places the dependency's entries for the same space ahead of the dependent's.

This has nothing to order yet. An invalidation event names one service, so
every entry in a plan belongs to that service, and the rule waits for the day
a plan can span two.

**Then cut.** `concurrency_limit × rate_per_s` against the deadline gives how
much can actually be fetched; the ordering above says what survives. Whatever
was cut shows up as `stats.coverage_of_demand` being less than 1, rather than
as silence.

## Lanes

`lane: "warm"` is the default: the executor stays within the manifest's
concurrency and rate limits and takes only spare capacity.

`lane: "urgent"` is a promotion the planner makes when the deadline is tight.
It still assumes the origin keeps interactive traffic — real misses, with
someone waiting — ahead of warming, which is what `lanes.interactive_priority`
in the manifest asserts.

## Determinism

The same inputs must produce the same plan, byte for byte. Not approximately
the same set of tiles in roughly that order: the same file.

- Entries are sorted by the total order `(metadata first, priority desc,
  service, qk, id)`. The tail of the ordering exists to break ties that
  floating point would otherwise break arbitrarily, and the quadkey in it does
  a second job: a quadkey sorts before the quadkeys it is a prefix of, so an
  ancestor precedes its descendants once their priorities are equal.
- Floating-point combination happens in a defined order, so that a sum is not
  at the mercy of what order the inputs happened to arrive in.
- Golden tests in [`tests/golden/`](../tests/golden) hold a set of inputs
  against the exact expected output.

This is not a preference for tidiness. A plan is reviewed by a person and
carries a cost estimate someone acts on; if re-running the planner produced a
different plan, neither the review nor the estimate would mean anything, and
`derived_from` would be a decoration rather than a claim anyone could check.

## The estimate

Computed with the plan, from the plan's own ordering. Four numbers, because
the decision needs both sides of a comparison.

| | How |
|---|---|
| **Time** | the larger of `Σ expected_gen_ms / concurrency_limit` and `tiles / rate_per_s` |
| **Money** | manifest `billing` counts × pricing table units |
| **Storage** | `Σ expected_bytes` added by the new epoch, alongside the old epoch's `reclaimable` bytes |
| **Opportunity** | the tiles left out, each at its cell's `p50_gen_ms` — the waiting interactive traffic does instead |

Time takes the larger of two limits because either can bind: many cheap tiles
run out of request rate, a few expensive ones run out of concurrency.

Opportunity counts tiles rather than requests, because a cold tile is generated
once and is then not cold. The number of people who wait is the number of tiles
nobody warmed — `tiles_observed` across the cells in scope, less what the plan
named — and each waits as long as its own cell measured.

Money keeps resource counts and unit prices apart on purpose: counts belong to
the service and change when its code changes, prices belong to the vendor and
change when the vendor feels like it. The plan records the pricing file's hash
so an old estimate can still be recomputed and checked.

### The marginal curve

The absolute cost matters less than where to stop. Since the entries are
totally ordered, the cumulative curve falls out at no extra cost, and
`estimate.marginal` reports it at several coverage levels.

Power-law demand puts a visible knee in that curve — 80% of the demand for
$3.20, the next 15% for another $6.60. That is what makes `--budget`,
`--deadline` and `--coverage` usable as the only three levers anyone needs,
instead of a coverage target picked by feel.

### Calibration

Estimates rest on measured `p50_gen_ms`, so only cells with no measurement
fall back to the manifest. Warming itself emits tile-demand events with
`origin: "warm"`, which means today's warming measures the tiles that
tomorrow's estimate is about.

`okibi estimate --compare <plan.json>` puts predictions against what actually
happened, so the model can be checked rather than trusted.

## Reserved

```
should_warm :: (digest_slice, tile_ref, epochs) -> { warm: bool, priority: f64 }
```

The same derivation for one tile instead of a set, for a delivery path asking
"is this worth revalidating" at request time. Reserved in the API, not
implemented.
