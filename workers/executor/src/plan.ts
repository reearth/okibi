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
