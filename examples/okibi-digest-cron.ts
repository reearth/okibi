// Taking the daily digest from inside the service's own Worker.
//
// The alternative is a scheduled CI job, which works and costs nothing on a
// public repository. What this buys is that a Cloudflare cron does not switch
// itself off after sixty quiet days, and that the service stops needing a
// second home for its scheduled work.
//
// The aggregation is not written here. `assembleDigest` is the same compiled
// function `okibi digest` runs, because which cell an unplaced request belongs
// to, how a tie between two equally hot tiles breaks, and what happens to a
// row that cannot be placed are all decisions that fail silently when two
// implementations disagree — and a digest that means something slightly
// different is a plan that warms somewhere slightly wrong.

import { assembleDigest, digestQueries } from "@reearth/okibi";

interface Env {
  /**
   * Bucket the digests are kept in, past Analytics Engine's three months.
   *
   * The service's own cache bucket is the natural home: a digest of one
   * service's demand is that service's, and the bucket already exists.
   */
  OKIBI_DIGESTS: R2Bucket;
  /** Token that may read the Analytics Engine SQL API. */
  OKIBI_CF_API_TOKEN: string;
  OKIBI_ACCOUNT_ID: string;
  /** This service's name, as it appears in its own events. */
  OKIBI_SERVICE: string;
}

export async function takeDigest(env: Env, date: string): Promise<void> {
  const config = { services: [env.OKIBI_SERVICE] };
  // The top-tiles query is per service, and this config names one, so it is
  // the one that comes back. An aggregator reading several would run the
  // cells query first and ask for top tiles once per service it found.
  const { cells, topTiles } = digestQueries(config, date);

  const [cellRows, tileRows] = await Promise.all([
    runSql(env, cells),
    runSql(env, topTiles),
  ]);

  const { records, skipped } = assembleDigest(cellRows, tileRows, date, 20);

  // Nothing is dropped quietly: a digest that covered less than it was asked
  // to would otherwise read as a quiet day.
  if (skipped.unknown_kind || skipped.unplaceable || skipped.cells_without_top) {
    console.warn("okibi: not everything became a record", skipped);
  }

  const jsonl = records.map((record) => JSON.stringify(record)).join("\n") + "\n";
  await env.OKIBI_DIGESTS.put(`okibi/digests/${date}.jsonl`, jsonl);

  console.log(`okibi: ${records.length} cells for ${date}`);
}

async function runSql(env: Env, sql: string): Promise<unknown[]> {
  const response = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${env.OKIBI_ACCOUNT_ID}/analytics_engine/sql`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${env.OKIBI_CF_API_TOKEN}`,
        "Content-Type": "text/plain",
      },
      body: sql,
    },
  );

  if (!response.ok) {
    throw new Error(`the SQL API answered ${response.status}: ${await response.text()}`);
  }

  const { data, rows } = (await response.json()) as { data: unknown[]; rows: number };

  // A gap between what the API counted and what came back as rows means the
  // query and the reader have drifted apart, which otherwise shows up as a
  // digest quietly missing most of its cells.
  if (rows !== data.length) {
    throw new Error(`the SQL API returned ${rows} rows and ${data.length} could be read`);
  }
  return data;
}

export default {
  async scheduled(event: ScheduledController, env: Env): Promise<void> {
    // Yesterday, in UTC. A day that is still being written to would be a
    // digest of part of a day, filed under the whole of it.
    const yesterday = new Date(event.scheduledTime - 24 * 60 * 60 * 1000);
    await takeDigest(env, yesterday.toISOString().slice(0, 10));
  },
};

// wrangler.toml:
//
//   [triggers]
//   crons = ["0 18 * * *"]        # 03:00 JST
//
//   [[r2_buckets]]
//   binding = "OKIBI_DIGESTS"
//   bucket_name = "reearth-papers"   # this service's own cache bucket
//
//   [vars]
//   OKIBI_SERVICE = "papers"
//
//   wrangler secret put OKIBI_CF_API_TOKEN
