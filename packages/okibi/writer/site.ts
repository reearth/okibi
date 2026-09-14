import type { HasHeaders } from "./origin.js";

/**
 * Which site asked for this tile, as a bare origin.
 *
 * A tile service is a dependency of other people's maps, and nothing else it
 * records says whose. Demand answers *what* is asked for and this answers
 * *who asks*, in the same row, which is the only place the two can be read
 * together.
 *
 * **An origin, never a full referrer.** `Referer` carries a page URL, and a
 * page URL carries paths, ids and query strings that belong to the site's
 * users rather than to the tile service — a search term, a document id, a
 * share token. Scheme and host answer the question; the rest is somebody
 * else's business being kept in a log nobody meant to make sensitive. The
 * cardinality argument points the same way: one row per site is a column a
 * query can group by, and one row per page is not.
 *
 * `Origin` is preferred because it is already only an origin, and because a
 * tile fetch is a cross-origin fetch — MapLibre, Cesium and a plain `<img>`
 * with crossorigin all send it. `Referer` is the fallback for the requests
 * that carry one and no `Origin`, reduced to its origin before it is kept.
 *
 * Empty for anything that sends neither, which is most non-browser clients:
 * a native app, a script, a crawler. Empty is an answer — "not a web page" —
 * rather than a gap, and it is recorded as one.
 */
export function siteOf(request: HasHeaders): string {
  const origin = request.headers.get("Origin");
  if (origin && origin !== "null") return normalise(origin);

  const referer = request.headers.get("Referer");
  if (referer) return normalise(referer);

  return "";
}

/**
 * Scheme and host, and nothing else.
 *
 * Built with `URL` rather than a regular expression so that a value this does
 * not understand comes back empty instead of coming back as whatever the
 * pattern happened to match. A header is something a client sends, which
 * means it is something a client can send anything in.
 */
function normalise(value: string): string {
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") return "";
    return url.origin;
  } catch {
    return "";
  }
}
