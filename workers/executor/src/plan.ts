// Turning a plan into work, and doing it.
//
// The executor understands nothing about tiles: a plan is a list of URLs in an
// order someone else decided, and warming is asking for them. What it is for
// is the one thing a CI job cannot do — outlast the six hours a job may run —
// so everything here is about getting the list into a queue and back out at a
// pace the origin will tolerate.

/**
 * The header that keeps a warm request out of the demand ledger.
 *
 * The value is a shared secret rather than a `1`: a mark anyone could send is
 * a way for anyone to remove their own requests from the ledger, and demand
 * that is not recorded is demand that is never warmed.
 */
export const WARM_HEADER = "X-Okibi-Warm";

export interface PlanEntry {
  url: string;
  service: string;
  priority: number;
  lane?: "warm" | "urgent";
  not_before?: string | null;
  expected_gen_ms?: number;
}

export interface WarmPlan {
  plan: string;
  entries: PlanEntry[];
}

/** One entry, on its way through the queue. */
export interface WarmMessage {
  url: string;
  service: string;
  lane: "warm" | "urgent";
}

export class NotAPlan extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NotAPlan";
  }
}

/** The plan versions this executor knows how to read. */
export const READS = ["okibi-warm-plan/1"];

export function messagesFor(body: unknown): WarmMessage[] {
  const plan = body as WarmPlan | null;
  if (!plan || typeof plan !== "object" || !Array.isArray(plan.entries)) {
    throw new NotAPlan("not a warm plan");
  }
  if (!READS.includes(plan.plan)) {
    // Refusing an unknown version rather than reading what looks familiar:
    // a plan whose fields moved would be warmed wrong and silently.
    throw new NotAPlan(
      `this executor reads ${READS.join(", ")}, not ${JSON.stringify(plan.plan)}`,
    );
  }

  return plan.entries.map((entry) => {
    if (!entry?.url || !entry.service) {
      throw new NotAPlan("an entry has no url or no service");
    }
    return {
      url: entry.url,
      service: entry.service,
      lane: entry.lane === "urgent" ? "urgent" : "warm",
    };
  });
}

/** How many of one service's tiles to have in flight at once. */
export type Limits = Record<string, number>;

export function concurrencyFor(limits: Limits, service: string): number {
  const limit = limits[service] ?? limits["*"] ?? 4;
  return Math.max(1, Math.floor(limit));
}

export interface Outcome {
  url: string;
  ok: boolean;
  status?: number;
  error?: string;
}

/**
 * Fetch a batch, at most `concurrency` at a time.
 *
 * Grouped by service because the limit is the service's: two origins in one
 * batch should not have to take turns with each other.
 */
export async function warmBatch(
  messages: WarmMessage[],
  limits: Limits,
  secret: string | undefined,
  fetcher: typeof fetch = fetch,
): Promise<Outcome[]> {
  const byService = new Map<string, WarmMessage[]>();
  for (const message of messages) {
    const queue = byService.get(message.service) ?? [];
    queue.push(message);
    byService.set(message.service, queue);
  }

  const results = await Promise.all(
    [...byService].map(([service, queue]) =>
      pool(queue, concurrencyFor(limits, service), (message) =>
        warmOne(message, secret, fetcher),
      ),
    ),
  );
  return results.flat();
}

async function warmOne(
  message: WarmMessage,
  secret: string | undefined,
  fetcher: typeof fetch,
): Promise<Outcome> {
  try {
    // Without the secret the tile still warms; only the ledger entry comes
    // out as organic. Warming anyway beats refusing over bookkeeping.
    const response = await fetcher(message.url, {
      headers: secret ? { [WARM_HEADER]: secret } : {},
    });
    return response.ok
      ? { url: message.url, ok: true, status: response.status }
      : { url: message.url, ok: false, status: response.status };
  } catch (error) {
    return {
      url: message.url,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/** How many of a plan's URLs to ask about before queueing the rest. */
export const SAMPLE = 3;

export interface Sampled {
  /** URLs the origin said are not there. The plan's fault. */
  wrong: string[];
  /** Asks that came back as an answer about the URL at all. */
  answered: number;
}

/**
 * Ask the origins whether a few of a plan's URLs exist.
 *
 * Here, on the way in, rather than left to the draining: a plan built from a
 * template, an id or an epoch that does not rebuild the service's URLs is
 * wrong in every entry, and queueing it spends a real request on each of them
 * to learn what three would have said.
 *
 * Here rather than in the service that made the plan, too. A Worker asking
 * for its own hostname goes out to the edge and comes back 522, so a service
 * planning its own warming cannot check its own URLs — this is a different
 * Worker on a different name, and can.
 *
 * Spread through the plan rather than taken from its head: the head is the
 * hottest cell, and a template that happens to work there can be wrong three
 * zoom levels down. A 5xx is not an answer about the URL and is counted
 * apart, because a check that cannot fail is worse than no check — it reads
 * as one that passed.
 */
export async function sample(
  messages: WarmMessage[],
  secret: string | undefined,
  fetcher: typeof fetch = fetch,
): Promise<Sampled> {
  const wrong: string[] = [];
  let answered = 0;
  const stride = Math.max(1, Math.ceil(messages.length / SAMPLE));

  for (let i = 0; i < messages.length && wrong.length < SAMPLE; i += stride) {
    const url = messages[i]?.url;
    if (!url) continue;
    try {
      const response = await fetcher(url, {
        method: "HEAD",
        headers: secret ? { [WARM_HEADER]: secret } : {},
      });
      if (response.status >= 500) continue;
      answered++;
      if (response.status >= 400) wrong.push(`${response.status} ${url}`);
    } catch {
      // Not an answer about the URL either.
    }
  }
  return { wrong, answered };
}

/** What one batch did, in the shape a log line should carry. */
export interface BatchSummary {
  warmed: number;
  failed: number;
  /** Per service, because a plan can span origins and one of them failing is
   *  a different thing from all of them failing. */
  services: Record<string, { warmed: number; failed: number }>;
  /** HTTP status, or `error` for a request that never got one. */
  statuses: Record<string, number>;
}

/**
 * Count a batch's outcomes.
 *
 * Separate from doing the work so that what gets logged can be asserted on.
 * A summary that drifted from the outcomes would be a log that reads as a
 * successful run of something that failed.
 */
export function summarise(
  messages: WarmMessage[],
  outcomes: Outcome[],
): BatchSummary {
  const summary: BatchSummary = {
    warmed: 0,
    failed: 0,
    services: {},
    statuses: {},
  };

  outcomes.forEach((outcome, i) => {
    const service = messages[i]?.service ?? "unknown";
    const per = (summary.services[service] ??= { warmed: 0, failed: 0 });

    if (outcome.ok) {
      summary.warmed++;
      per.warmed++;
    } else {
      summary.failed++;
      per.failed++;
    }

    const key = outcome.status !== undefined ? String(outcome.status) : "error";
    summary.statuses[key] = (summary.statuses[key] ?? 0) + 1;
  });

  return summary;
}

/** Run `work` over `items`, `width` at a time, keeping the input order. */
async function pool<T, R>(
  items: T[],
  width: number,
  work: (item: T) => Promise<R>,
): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;

  const runners = Array.from({ length: Math.min(width, items.length) }, () =>
    (async () => {
      while (true) {
        const index = next++;
        const item = items[index];
        if (item === undefined) return;
        results[index] = await work(item);
      }
    })(),
  );

  await Promise.all(runners);
  return results;
}
