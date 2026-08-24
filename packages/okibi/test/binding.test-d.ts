// The writer has to accept the real binding.
//
// `Dataset` here is a hand-written stand-in for `AnalyticsEngineDataset`, so
// that a service importing the writer does not have to bring a runtime's whole
// type package with it. A stand-in that has drifted from the thing it stands
// in for is a compile error in someone else's repository, so it is checked
// here instead — this file is typechecked, never run.

import type { AnalyticsEngineDataset } from "@cloudflare/workers-types";

import { type Dataset, createWriter } from "../writer/index.js";
import type { EpochsFile } from "../writer/index.js";

declare const binding: AnalyticsEngineDataset;

// The assignment is the assertion.
const dataset: Dataset = binding;

const epochs: EpochsFile = {
  service: "papers",
  tilesets: { "style-aoi-04": { source: "s", algo: "a", param: "p" } },
};

const writer = createWriter({ dataset, epochs });

writer.write({
  tileset: "style-aoi-04",
  kind: "content",
  id: "14/14552/6451",
  qk: "13300211231022",
  cacheStatus: "miss",
  fmt: "png",
  origin: "organic",
  genMs: 34120,
  bytes: 88231,
  z: 14,
});

// And a service that binds it the way wrangler types generate it.
interface Env {
  TILE_DEMAND: AnalyticsEngineDataset;
}

declare const env: Env;
createWriter({ dataset: env.TILE_DEMAND, epochs }).write({
  tileset: "style-aoi-04",
  kind: "tileset",
  id: "meta.json",
  cacheStatus: "hit",
  fmt: "json",
  origin: "warm",
  genMs: 0,
  bytes: 18220,
});
