import { readFileSync } from "node:fs";

import initWeb, {
  analyze_vref as analyzeWeb,
  rule_catalog as catalogWeb,
} from "../pkg-web/sous_chef_web.js";
import {
  analyze_vref as analyzeBundler,
  rule_catalog as catalogBundler,
} from "../pkg-bundler/sous_chef_web.js";

// The Review-Depth-mapped set, pinned so a rule cannot gain or lose a mapped
// profile without this smoke failing. `punct.spacing-anomaly` was the original
// third member; it was deleted and its domain absorbed by
// `uni.nonletter-usage-anomaly`, which carries the mapped profile now (ADR 0071).
const expectedMapped = new Set([
  "case.sentence-initial-lowercase",
  "case.inconsistent-word-casing",
  "uni.nonletter-usage-anomaly",
]);

function assertCatalog(catalog, label) {
  if (catalog.review_depth.default !== 50) {
    throw new Error(`${label}: Review Depth default drifted`);
  }
  if (catalog.review_depth.minimum !== 0 || catalog.review_depth.maximum !== 100) {
    throw new Error(`${label}: Review Depth bounds drifted`);
  }
  const mapped = new Set(
    catalog.cards
      .filter((card) => card.review_control === "mapped")
      .map((card) => card.code),
  );
  if (mapped.size !== expectedMapped.size || [...mapped].some((code) => !expectedMapped.has(code))) {
    throw new Error(`${label}: mapped catalog set drifted: ${[...mapped].join(",")}`);
  }
}

function assertAnalysis(analyze, label) {
  const target = { keys: ["GEN 1:1"], texts: ["In the beginning."] };
  const bytes = analyze({ target, config: { review: { depth: 51 } } });
  if (!(bytes instanceof Uint8Array) || bytes.length < 32) {
    throw new Error(`${label}: valid Review Depth analysis did not return a snapshot`);
  }
  try {
    analyze({ target, config: { review: { depth: 101 } } });
  } catch (error) {
    if (!String(error).includes("review depth must be an integer")) {
      throw new Error(`${label}: invalid depth returned the wrong error: ${error}`);
    }
    return;
  }
  throw new Error(`${label}: invalid Review Depth was accepted`);
}

await initWeb({
  module_or_path: readFileSync(new URL("../pkg-web/sous_chef_web_bg.wasm", import.meta.url)),
});
assertCatalog(catalogWeb(), "web");
assertAnalysis(analyzeWeb, "web");
assertCatalog(catalogBundler(), "bundler");
assertAnalysis(analyzeBundler, "bundler");
console.log("Review Depth generated-package smoke passed for web and bundler");
