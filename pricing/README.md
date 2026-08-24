# Pricing tables

One file per profile per month, matching
[`spec/schema/pricing-table.schema.json`](../spec/schema/pricing-table.schema.json).
A plan records the hash of the table it used, so an estimate from last year can
still be recomputed and checked.

**These files are append-only.** A price change is a new file for a new month.
Editing an old one silently changes what every estimate that cites it meant,
and the hash in those plans then matches nothing.

The prices here are transcribed by hand from the vendor's public pricing and
are not fetched from anywhere. Before trusting an estimate in a currency that
matters, check the numbers against the vendor's page for that month — and if
they have moved, add a file rather than correcting this one.

A price okibi does not find in a table is treated as zero, which is right for
R2's egress and wrong for anything the table simply forgot. The `usd` in an
estimate is only as complete as the table it was priced with.
