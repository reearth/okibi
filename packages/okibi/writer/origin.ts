import type { Origin } from "./types.js";

/**
 * The header okibi's executor sends, and the only thing that distinguishes a
 * warm request from anyone else's.
 */
export const WARM_HEADER = "X-Okibi-Warm";

/** Just enough of `Request` to ask about a header. */
export interface HasHeaders {
  headers: { get(name: string): string | null };
}

/**
 * Whether this request is okibi warming a tile, or somebody wanting one.
 *
 * The distinction has to be made at the edge of the service, because by the
 * time it reaches the ledger the two are indistinguishable — and counting
 * warm requests as demand is a feedback loop, not a rounding error.
 *
 * It also has to be unforgeable, which is why the header carries a secret
 * rather than a `1`. A mark anyone could send is a way for anyone to remove
 * their own requests from the ledger, and demand that is not recorded is
 * demand that is never warmed.
 *
 * With no secret configured nothing is warm. That errs toward counting
 * okibi's own traffic as demand — a bounded, visible error — rather than
 * toward a ledger anyone can edit.
 */
export function originOf(request: HasHeaders, secret: string | undefined): Origin {
  if (!secret) return "organic";
  return matches(request.headers.get(WARM_HEADER), secret) ? "warm" : "organic";
}

/** The headers okibi's own executor sends. */
export function warmHeaders(secret: string): Record<string, string> {
  return { [WARM_HEADER]: secret };
}

/**
 * Compared without an early exit, so that how long the answer took does not
 * say how much of the secret was right.
 */
function matches(given: string | null, expected: string): boolean {
  if (given === null || given.length !== expected.length) return false;

  let difference = 0;
  for (let i = 0; i < expected.length; i++) {
    difference |= given.charCodeAt(i) ^ expected.charCodeAt(i);
  }
  return difference === 0;
}
