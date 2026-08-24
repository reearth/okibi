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

    return Response.json({ queued: messages.length });
  },

  async queue(batch: MessageBatch<WarmMessage>, env: Env): Promise<void> {
    const limits = readLimits(env.OKIBI_LIMITS);
    const outcomes = await warmBatch(
      batch.messages.map((message) => message.body),
      limits,
      env.OKIBI_WARM_SECRET,
    );

    outcomes.forEach((outcome, i) => {
      const message = batch.messages[i];
      if (!message) return;

      // A tile that did not warm is a tile that will be generated on demand,
      // which is what would have happened anyway. It is worth one retry and
      // not worth blocking the rest of the queue for.
      if (outcome.ok) {
        message.ack();
      } else {
        message.retry();
      }
    });
  },
};

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
