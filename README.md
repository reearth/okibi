# okibi

Warm the tiles that people actually ask for, after a cache invalidation kills them.

> ⚠️ Early work in progress. Wired end to end and running against live demand;
> no invalidation has happened yet that somebody did not arrange — see
> [Status](#status).

## The problem

Generating map tiles on demand beats pre-generating them on freshness: change
the source data, the algorithm or a parameter, and the next request gets the
new answer without anyone rebuilding a pyramid. The cost is that invalidation
leaves the cache cold, and the next visitor pays the full generation time.

How much that hurts depends on the service. Re:Earth Terrain takes about 2.4
seconds on a miss where a hit takes 86 milliseconds. Re:Earth Papers, which
paints its tiles, takes seconds to tens of seconds. For Papers that is past
what anyone will sit through, and no amount of making the generator faster
removes the problem, because the problem is structural to generating on demand.

So something has to re-generate the important tiles *before* anyone asks. The
question is which ones — and that question has a good answer, because tile
demand is not uniform. It concentrates in cities, it follows a power law, and
a request for a deep tile implies requests for its ancestors. Warming the head
of the distribution and leaving the long tail to on-demand generation is cheap
even when the invalidation was global: the tiles with any observed demand at
all are a few percent of the space.

## What okibi is

A shared component that sits outside the services it warms. It never reaches
into a service: warming is just `GET`, so the on-demand path *is* the
generator, and okibi needs to know nothing about how a tile gets made.

It is four things, and only the last one is code:

| | | |
|---|---|---|
| ① | tile-demand vocabulary | what a service writes per tile request |
| ② | backend binding | where those attributes physically land (first: Workers Analytics Engine) |
| ③ | demand-digest format | aggregated demand; the planner's only input |
| ④ | the planner | `plan(digest, invalidation, manifest) -> warm_plan` |

①②③ are portable specifications with no cloud vendor in them. Only ② is
swapped when the log backend changes.

The planner is a pure function, and the plan it produces is a JSON file you can
store, diff and review — the same split terraform makes between plan and apply.
Cost falls out of the same call: once the plan exists, the time, the money, the
storage and the latency users would have eaten without it are all already
computed. Put that on a pull request and changing one epoch string stops being
free — cache economics become something a reviewer can see.

## What's here

| | |
|---|---|
| [`spec/`](spec/README.md) | ①②③, and the planner's algorithm, with the JSON Schemas that are enforced |
| [`crates/okibi-core`](crates/okibi-core) | the planner and the documents it reads |
| [`crates/okibi-qk`](crates/okibi-qk) | the projection that makes unlike tile schemes comparable |
| [`crates/okibi-cli`](crates/okibi-cli) | `digest`, `plan`, `warm`, `invalidation`, `report`, `diff`, `explain` |
| [`packages/okibi`](packages/okibi) | what a service imports: the writer, the projection, the digest, the planner |
| [`workers/executor`](workers/executor) | drains a plan from a queue, for warming that outlasts a CI job |
| [`actions/`](actions) | `digest` daily, `plan` on a pull request, `warm` after a deploy, `watch` for what no deploy caused |

A service that runs a Worker can do the daily digest and the watching from its
own cron instead of from CI, using the same compiled planner. A Cloudflare
cron does not switch itself off after sixty quiet days, which for a watch is
the difference between noticing and looking as though there was nothing to
notice. The actions are for everywhere else.

Warming is hours of waiting on IO. A Worker bills CPU time, so waiting there
costs almost nothing; a CI job is a rented machine sitting idle, and stops at
six hours either way. That is what the executor is for, and why a plan goes to
it whenever there is one.

## Status

Running. Re:Earth Terrain, Buildings and Papers have written tile-demand
events since 2026-08-24; each takes its own digest from its own Worker cron
and keeps it in its own bucket; the executor is deployed and drains plans from
a queue; and Buildings warms from its repository when an epoch moves, while
Papers watches for the epochs that move without anyone pushing.

What has not happened is a plan warmed against an invalidation nobody
arranged. Every one so far has been written by hand to make something run.

### What meeting real data cost

Every one of these was a wrong answer that looked exactly like a right one,
which is the failure this whole design is arranged against — and not one of
them was found by a test here.

Three were in the queries, and only Analytics Engine could say so. It rejects
an `IF()` whose branches are a double and an integer, so the digest failed on
its first run. The top-tiles row limit, spent across every service at once,
went almost entirely to the busiest: Terrain outweighs Papers by two orders of
magnitude, so Papers came back with no top tiles in any cell and nothing to
plan from. And the generation quantiles were taken over hits as well as
misses, so a cell that mostly hits reported a median generation of zero — free
rather than unmeasured.

Three were in what a plan is built from. A document with no coordinates —
Buildings' root tileset — was given the tile URL template, which is built out
of coordinates, and the URL it produced 404s while sorting first. Papers
reported ids that rebuilt nothing: no format extension, two URL shapes under
one template, and parameters that live in a query string. And an id built from
the whole query string let a smoke test's cache-buster split one tile into as
many ids as anyone cared to invent.

One was in the checking. Papers' watch asks whether a few of its plan's URLs
exist before handing them over — and a Worker asking for its own hostname goes
out to the edge and comes back 522, so three timeouts read as three passes and
569 URLs that every one answered 404 were handed over as verified. A check
that cannot fail is worse than no check. It is the executor that asks now,
from a different name, before anything is queued.

`okibi plan --verify` came out of the second group and would have caught all
of it. The last one is why the executor asks too.

### What the ledger is worth

A demand digest is a lower bound. An event is written by the service, so a
request the edge answers before the service runs is a request nothing records
— 62% of Terrain's traffic on 2026-08-26, 22% of Papers', 0.5% of Buildings'.
What an edge absorbs is what repeats within one colo, which is the head of the
distribution, so a digest describes demand flatter than it is. See
[`spec/bindings/wae-1.md`](spec/bindings/wae-1.md) for why that undersells
warming rather than misdirecting it, and why bypassing the edge would fix the
ledger by defeating the purpose.

`@reearth/okibi` is published to npm. Nothing is published to crates.io.

## Using it

A service needs two files and one call.

`okibi.epochs.json` holds the epoch strings, and the service builds its cache
keys from the same file — which is what keeps a demand event's epochs
byte-identical to the key it was cached under.

`okibi.manifest.json` is everything okibi is allowed to know: the URL template
that regenerates a tile, what the origin will tolerate, and what a generation
costs.

Then [`@reearth/okibi/writer`](packages/okibi) writes one event per tile
request. Everything after that happens outside the service —
[`examples/service-workflow.yml`](examples/service-workflow.yml) is the whole
of it.

**okibi keeps no roster.** No file here lists which services exist. The digest
aggregates whatever is in the dataset, because writing events is already how a
service joins; the warming is scheduled by each service from its own
repository, because a service knows its own name. A central list would be a
second answer to who exists, and wrong the day someone was added to one and
not the other.

What is here is the vocabulary, the planner and the prices — the parts that
are the same wherever okibi is installed. The services named further up are
the ones it was built for, not ones it is configured with.

## The name

**okibi** (熾火) is the fire that stays under the ash after the flames have
gone out — embers, still hot, ready to be brought back with very little work.

Which is the job. The cache is not kept burning, and it is not allowed to go
cold either; after an invalidation, the heat that is still worth having is put
back where it was.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your
option.
