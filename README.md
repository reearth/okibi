# okibi

Warm the tiles that people actually ask for, after a cache invalidation kills them.

> ⚠️ Early work in progress. Nothing here runs yet — see [Status](#status).

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
| [`packages/okibi`](packages/okibi) | the projection as wasm, for services |
| [`packages/okibi-writer`](packages/okibi-writer) | the one call a service makes per tile request |
| [`workers/executor`](workers/executor) | drains a plan from a queue, for warming that outlasts a CI job |
| [`actions/`](actions) | `plan` on a pull request, `warm` after a deploy |

Warming is hours of waiting on IO. A Worker bills CPU time, so waiting there
costs almost nothing; a CI job is a rented machine sitting idle, and stops at
six hours either way. That is what the executor is for, and why a plan goes to
it whenever there is one.

## Status

All four pieces exist and run: the [specifications](spec/README.md), the
writer a service calls, the planner, the executor, and the actions that put a
plan on a pull request and then fetch it.

What has not happened is any of it meeting real data. No service writes
tile-demand events yet, so no digest has ever been taken from a live dataset —
which also means the Analytics Engine SQL in `okibi digest` has never run
against Analytics Engine. `okibi digest --print-sql` exists to be read before
it is trusted.

Nothing is published to crates.io or npm.

## Using it

A service needs two files and one call.

`okibi.epochs.json` holds the epoch strings, and the service builds its cache
keys from the same file — which is what keeps a demand event's epochs
byte-identical to the key it was cached under.

`okibi.manifest.json` is everything okibi is allowed to know: the URL template
that regenerates a tile, what the origin will tolerate, and what a generation
costs.

Then [`@reearth/okibi-writer`](packages/okibi-writer) writes one event per tile
request. Everything after that happens outside the service —
[`examples/service-workflow.yml`](examples/service-workflow.yml) is the whole
of it.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your
option.
