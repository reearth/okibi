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
 */
export function originOf(request: HasHeaders): Origin {
  return request.headers.get(WARM_HEADER) ? "warm" : "organic";
}

/** The headers okibi's own executor sends. */
export function warmHeaders(): Record<string, string> {
  return { [WARM_HEADER]: "1" };
}
