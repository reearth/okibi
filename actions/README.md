# Actions

Four, one per moment something has to happen.

| | when | what |
|---|---|---|
| [`digest`](digest) | daily | aggregate a day of events and keep it past the log backend's retention |
| [`plan`](plan) | a pull request | say what the cache change in this commit costs, and warm nothing |
| [`warm`](warm) | after a deploy | fetch the plan, in the executor if there is one |
| [`watch`](watch) | on a schedule, per service | notice a service that went cold with nothing pushed, and warm it |

All four are triggered from the repository of the service being warmed, which
is where the thing knows its own name, and each keeps its digests in that
service's own cache bucket. okibi schedules nothing, keeps no list of who
exists, and owns no storage.

`digest` has a second route that does not involve CI at all: a service that
runs a Worker can take its own digest from a cron trigger, using the same
aggregation compiled to wasm —
[`examples/okibi-digest-cron.ts`](../examples/okibi-digest-cron.ts). Prefer
that where there is a Worker: a Cloudflare cron does not switch itself off
after sixty quiet days, and waiting on an HTTP call costs it nothing. This
action is for everywhere else.

[`examples/service-workflow.yml`](../examples/service-workflow.yml) is what a
service adds to use `plan`, `warm` and `watch` together.

## Versions

Reference them by release tag — `reearth/okibi/actions/plan@v0.6.0`.

The major tag these were meant to be referenced by, `@v1`, does not exist and
should not be created yet: the release workflow fires on `v*` and would read
`v1` as a version to publish to npm. It arrives with 1.0, and will track the
major version of the specifications these implement — a plan produced by `@v1`
being an `okibi-warm-plan/1`.

Each action builds okibi from source, because nothing is released yet. That is
what the cache in every one of them is holding down, and it becomes a download
the day there is something to download.
