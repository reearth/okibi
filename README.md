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

## Status

Scaffolding only. The specifications are drafted in `tmp/IDEA.md` and have not
been split into `spec/` yet; no crate has an implementation; nothing is
published anywhere.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your
option.
