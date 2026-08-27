// The queue-consuming executor.
//
// A CI job stops at six hours. A plan for a global source change does not, so
// it is handed here instead: the plan goes into a queue, and the queue is
// drained at whatever pace the origins tolerate for as long as it takes.
//
// Nothing about warming is decided here. The order was decided by the planner,
// and this reads it from the top.

import {
  type Limits,
  NotAPlan,
  type WarmMessage,
  messagesFor,
  summarise,
  warmBatch,
} from "./plan.js";

export interface Env {
  /** The queue plans are split into. */
  WARM_QUEUE: Queue<WarmMessage>;
  /** Shared secret the plan endpoint is called with. */
  OKIBI_EXECUTOR_TOKEN: string;
  /** JSON: how many of each service's tiles to have in flight. */
  OKIBI_LIMITS?: string;
  /**
   * The secret a service checks before believing a request is okibi's.
   *
   * Shared with every service this warms. Without it the tiles still warm,
   * but each one is counted as demand — which is how warming becomes its own
   * evidence.
   */
  OKIBI_WARM_SECRET?: string;
}

/** The most messages one `sendBatch` may carry. */
const BATCH = 100;

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (request.method === "GET" && url.pathname === "/health") {
      return new Response("ok\n");
    }

    if (request.method !== "POST" || url.pathname !== "/plans") {
      return new Response("not found\n", { status: 404 });
    }

    if (!authorised(request, env.OKIBI_EXECUTOR_TOKEN)) {
      return new Response("unauthorised\n", { status: 401 });
    }

    let messages: WarmMessage[];
    try {
      messages = messagesFor(await request.json());
    } catch (error) {
      const why = error instanceof NotAPlan ? error.message : "unreadable body";
      return new Response(`${why}\n`, { status: 400 });
    }

    for (let i = 0; i < messages.length; i += BATCH) {
      await env.WARM_QUEUE.sendBatch(
        messages.slice(i, i + BATCH).map((body) => ({ body })),
      );
    }

    // A plan arriving is the start of hours of work that nothing else
    // announces. Without this line, the only evidence a plan was ever
    // accepted is the queue draining.
    console.log("okibi: queued a plan", {
      queued: messages.length,
      services: countBy(messages, (message) => message.service),
      lanes: countBy(messages, (message) => message.lane),
      warmSecret: env.OKIBI_WARM_SECRET ? "set" : "MISSING",
    });

    return Response.json({ queued: messages.length });
  },

  async queue(batch: MessageBatch<WarmMessage>, env: Env): Promise<void> {
    const limits = readLimits(env.OKIBI_LIMITS);
    const bodies = batch.messages.map((message) => message.body);
    const outcomes = await warmBatch(bodies, limits, env.OKIBI_WARM_SECRET);

    // What warmed is also written by the services themselves, as demand with
    // `origin: "warm"`. What is only here is what did *not* warm: a request
    // that never reached a handler wrote no event anywhere, so a tile okibi
    // gave up on would otherwise leave no trace at all.
    console.log("okibi: warmed a batch", summarise(bodies, outcomes));

    outcomes.forEach((outcome, i) => {
      const message = batch.messages[i];
      if (!message) return;

      // A tile that did not warm is a tile that will be generated on demand,
      // which is what would have happened anyway. It is worth one retry and
      // not worth blocking the rest of the queue for.
      if (outcome.ok) {
        message.ack();
        return;
      }

      // Named individually, because the summary says how many failed and this
      // says which — and a plan whose failures are all one origin is a
      // different problem from one whose failures are scattered.
      console.warn("okibi: did not warm", {
        url: outcome.url,
        service: message.body.service,
        status: outcome.status,
        error: outcome.error,
        attempt: message.attempts,
      });
      message.retry();
    });
  },
};

/** How many of each key there are, for a log line. */
function countBy<T>(items: T[], key: (item: T) => string): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const item of items) {
    const k = key(item);
    counts[k] = (counts[k] ?? 0) + 1;
  }
  return counts;
}

/** Constant-time enough for a shared secret in a header. */
export function authorised(request: Request, token: string): boolean {
  if (!token) return false;

  const given = request.headers.get("Authorization") ?? "";
  const expected = `Bearer ${token}`;
  if (given.length !== expected.length) return false;

  let difference = 0;
  for (let i = 0; i < expected.length; i++) {
    difference |= given.charCodeAt(i) ^ expected.charCodeAt(i);
  }
  return difference === 0;
}

export function readLimits(raw: string | undefined): Limits {
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    return typeof parsed === "object" && parsed ? (parsed as Limits) : {};
  } catch {
    // A malformed limit is not a reason to stop warming; it is a reason to
    // fall back to the cautious default.
    return {};
  }
}
