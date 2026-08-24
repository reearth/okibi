# Pricing tables

One file per profile per month, matching
[`spec/schema/pricing-table.schema.json`](../spec/schema/pricing-table.schema.json).
A plan records the hash of the table it used, so an estimate from last year can
still be recomputed and checked.

## What a profile names

A profile is a billing context, not a product. `cloudflare` prices `cpu_ms`
and `subrequest`, which are Workers, alongside `storage_class_a` and
`egress_byte`, which are R2 — because one generation spends all four, and a
manifest carries one `pricing_profile` for the service that does the spending.
Naming it after any single product would be narrower than what the file holds.

What a profile does have to distinguish is anything that changes the prices:
a different vendor, a different account tier, a different region. Split it when
one of those differs, not when a product does.

Adding a product means adding keys, not files. The units here and the
`per_gen` counts in a manifest are the same key space, so a service that
renders in a container prices `container_vcpu_s` by naming it in both places —
no schema anywhere has to learn what a container is.

Key a unit the way the vendor's price list keys it. Cloudflare bills a
container by vCPU-second and GiB-second rather than by instance type, so those
are the keys here; a key that cannot be found on the cited page is a key
somebody invented.

**These files are append-only.** A price change is a new file for a new month.
Editing an old one silently changes what every estimate that cites it meant,
and the hash in those plans then matches nothing.

Every table names its `source` and the day it was `retrieved`, and the schema
requires both. The failure this directory is most exposed to is not a stale
price but a plausible one — a number that looks right, prices a plan, and came
from nobody's price list. Citing the page is what makes disagreeing with it
possible.

So: read the numbers off the vendor's page, put the URL in `source`, and if a
price has moved, add a file rather than correcting this one.

A price okibi does not find in a table is treated as zero, which is right for
R2's egress and wrong for anything the table simply forgot. The `usd` in an
estimate is only as complete as the table it was priced with.
