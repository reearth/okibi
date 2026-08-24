# The okibi specifications

What a service writes, what the planner reads, and what a plan is — enough to
write another implementation of any part against, or to swap the log backend
underneath without touching anything else.

| | |
|---|---|
| [The tile-demand vocabulary](tile-demand.md) | the attributes a service writes per tile request, and the rules for writing them |
| [Bindings](bindings/) | where those attributes physically land. First and so far only: [Workers Analytics Engine](bindings/wae-1.md) |
| [The demand digest](demand-digest.md) | aggregated demand — the planner's only view of what anyone asked for |
| [The okibi contract](okibi-contract.md) | the three documents around the planner: a service manifest, an invalidation event, a warm plan |
| [The planner](planner.md) | how a plan is derived, and what makes two runs agree |
| [`schema/`](schema/) | the same documents as JSON Schema, which is what actually gets enforced |
| [`examples/`](examples/) | one valid document per schema, checked in CI |

The reasoning is not here. This says what the formats *are*, and the case for
warming tiles at all is not something an implementer of any one part should
have to read. A specification that argued with itself would be two documents
pretending to be one.

## What is normative

**The vocabulary is the original; a binding is a copy of it.** An attribute
means what [tile-demand.md](tile-demand.md) says it means. Where it sits —
which column, which blob index, which log line — is a binding's business, and
a binding is allowed to be replaced. This is the whole reason the two are
separate documents: adding a ClickHouse binding, or a "write JSONL to a
bucket" binding, must not be an edit to the vocabulary.

**JSON Schema is the original for the shapes.** Every document type here has a
file under [`schema/`](schema/), and the tables in these pages describe those
schemas rather than standing in for them. When prose and schema disagree, the
schema is what runs.

**The planner reads digests and nothing else.** It never reaches a log
backend. That is what keeps the backend swappable and the planner testable:
its inputs are three JSON documents, and its output is a fourth.

## Versioning

Each document type carries its own version in its first field — `"digest":
"tile-demand-digest/1"`, `"plan": "okibi-warm-plan/1"`. A breaking change
raises that number, so a reader in a mixed period can branch on the version it
was handed rather than guessing from the shape.

Bindings are numbered independently of the vocabulary, because they change for
their own reasons: `wae-1` is the first binding of the vocabulary to Workers
Analytics Engine, and a second binding starts at its own `1`.

Analytics Engine has no migrations, so the WAE binding's version also names
its dataset (`tile_demand_1`). A schema change there is a new dataset, a
period of writing to both, and then a reader cutover — not an alter.
