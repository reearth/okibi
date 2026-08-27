# okibi executor

Drains a [warm plan](../../spec/okibi-contract.md#warm-plan) from a queue.

It understands nothing about tiles. A plan is a list of URLs in an order
someone else decided, and warming is asking for them — the on-demand path is
the generator, so the request is the whole of the work. This is a conforming
executor:

```sh
jq -r '.entries[].url' plan.json | xargs -P 4 -n 1 curl -sf -o /dev/null
```

What this adds is the two things that pipe cannot do.

**It outlasts a CI job.** A job stops at six hours. A plan for a global source
change does not, and warming is hours of waiting on IO — which a Worker bills
at nearly nothing, because it bills CPU time rather than wall time. The same
wait in a job is a two-core machine sitting idle.

**It marks its requests.** Every fetch carries `X-Okibi-Warm: <secret>`, and a
service writes `tile.origin: "warm"` when the value matches. Without that, what
okibi warmed becomes tomorrow's demand and okibi warms it again — a loop that
ends with the plan describing the planner's own history.

## Interface

`POST /plans`, with `Authorization: Bearer <OKIBI_EXECUTOR_TOKEN>` and a warm
plan as the body. The entries go onto the queue and the response says how many:

```json
{ "queued": 4210 }
```

A plan whose version this executor does not read is refused rather than
interpreted. A plan whose fields had moved would be warmed wrong, and warming
the wrong thing looks exactly like warming the right thing.

`GET /health` answers `ok`.

## Configuration

| | |
|---|---|
| `OKIBI_EXECUTOR_TOKEN` | secret. What `POST /plans` is authenticated with |
| `OKIBI_WARM_SECRET` | secret. What a service checks before believing a request is okibi's |
| `OKIBI_LIMITS` | var. JSON, how many of each service's tiles to have in flight: `{"papers":4,"*":8}` |

`OKIBI_LIMITS` should match the `concurrency_limit` each service declares in
its manifest. It is the floor under the ceiling that `max_batch_size` and
`max_concurrency` in [`wrangler.jsonc`](wrangler.jsonc) set: those bound how
much work exists at once, this bounds how much of it lands on one origin.

```sh
wrangler secret put OKIBI_EXECUTOR_TOKEN
wrangler secret put OKIBI_WARM_SECRET
wrangler deploy
```

The two secrets are set once and survive every deploy after it, which is why
they are not in a workflow: a second copy of a shared secret is a second place
it can be read back out of.

The deploy itself is not by hand for this repository's own installation —
[`deploy-executor.yml`](../../.github/workflows/deploy-executor.yml) runs it
when anything under `workers/executor/` reaches `main`. A Worker deployed by
hand is a Worker that eventually runs whatever was last on somebody's laptop.
A fork's executor is a fork's business; the workflow checks which repository
it is in rather than failing a fork's push for want of a secret.

## Failures

A message is retried once and then dropped to the dead-letter queue. A tile
that will not generate twice gets generated on demand instead, which is where
it started, and holding the queue for it would cost the tiles behind it for
nothing.

## What it leaves behind

Two lines per batch in the Worker's logs, plus one for each plan accepted:

```
okibi: queued a plan     { queued: 1135, services: {…}, lanes: {…}, warmSecret: "set" }
okibi: warmed a batch    { warmed: 18, failed: 2, services: {…}, statuses: { "200": 18, "503": 2 } }
okibi: did not warm      { url: …, service: …, status: 503, attempt: 1 }
```

What warmed is recorded twice over — the services write it themselves, as
demand carrying `origin: "warm"` — so `blob13 = 'warm'` in the dataset answers
"what did okibi warm" for as long as the events are kept.

What is **only** here is what did not warm. A request that never reached a
handler wrote no event anywhere, so a tile okibi gave up on would otherwise
leave no trace at all: not in the ledger, which never saw it, and not in the
plan, which says what was meant to happen rather than what did.
