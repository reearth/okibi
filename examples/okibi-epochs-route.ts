// What a service adds so okibi can see what it is currently using.
//
// Two of the three services resolve part of their cache key at request time —
// Buildings picks up the latest Overture release, Terrain revalidates against
// Mapterhorn — so a deploy is not the only thing that invalidates them. This
// endpoint is how a change that nobody pushed becomes visible.
//
// It is public on purpose. Everything in it is already derivable: Buildings
// carries its renderer version in the URL, Terrain's ETag is built from the
// tileset version, and both publish their upstream sources in /attribution.
// Authenticating it would hide nothing and would create a door that someone
// later puts something genuinely secret behind.
//
// Which is a constraint rather than an observation: epoch strings have to stay
// publishable. If one ever needs to carry something that is not — a licensed
// dataset name, a customer identifier — that is a sign to rename the epoch,
// not to add a token.

import epochs from "../okibi.epochs.json";

export async function handleOkibiEpochs(env: Env): Promise<Response> {
  return Response.json(
    {
      ...epochs,
      tilesets: {
        ...epochs.tilesets,
        // Where an epoch is resolved at runtime, the resolved value is what
        // goes here — the file only holds what is known at build time.
        "overture-global": {
          ...epochs.tilesets["overture-global"],
          source: await resolveLatestRelease(env),
        },
      },
    },
    {
      headers: {
        // Polled every few hours, and the answer changes far less often than
        // that.
        "cache-control": "public, max-age=300",
      },
    },
  );
}
