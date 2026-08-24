# Actions

Four, one per moment something has to happen.

| | when | what |
|---|---|---|
| [`digest`](digest) | daily, centrally | aggregate a day of events and keep it past the log backend's retention |
| [`plan`](plan) | a pull request | say what the cache change in this commit costs, and warm nothing |
| [`warm`](warm) | after a deploy | fetch the plan, in the executor if there is one |
| [`watch`](watch) | on a schedule, per service | notice a service that went cold with nothing pushed, and warm it |

Three of the four are scheduled or triggered from the service's own
repository, because that is where the thing being warmed knows its own name.
`digest` is the exception: one dataset indexed by service means one query for
all of them, so it runs once wherever the installation keeps its credentials —
which for now is [this repository's own schedule](../.github/workflows/digest.yml).

[`examples/service-workflow.yml`](../examples/service-workflow.yml) is what a
service adds to use `plan`, `warm` and `watch` together.

## Versions

Reference them by major tag — `reearth/okibi/actions/plan@v1` — which tracks
the major version of the specifications they implement. A plan produced by
`@v1` is an `okibi-warm-plan/1`.

Each action builds okibi from source, because nothing is released yet. That is
what the cache in every one of them is holding down, and it becomes a download
the day there is something to download.
