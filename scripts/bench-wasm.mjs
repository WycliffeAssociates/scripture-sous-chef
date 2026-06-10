// Wasm-side counterpart of `cargo bench -p ssc-core` — the same NT
// corpus through the real `analyze_vref` boundary (pkg-web build run
// under Node), so the number includes JS→wasm marshaling of the verse
// map and UTF-16 projection of findings, exactly what the editor pays.
//
// Usage:
//   npm run build:wasm          # if pkg-web is stale
//   npm run bench:wasm          # defaults to corpora/bem_reg
//   node scripts/bench-wasm.mjs corpora/en_ulb
//
// Compare against `analyze/nt` (native serial) and `analyze/nt_rayon`
// in documentation/calibration/2026-06-09-perf-baseline.md.

import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const corpus = process.argv[2] ?? "corpora/bem_reg";

// The naive USFM loader lives in Rust dev tooling; reuse it for parity.
const dump = spawnSync(
  "cargo",
  ["run", "--release", "-p", "ssc-core", "--example", "calibrate", "--", "--json", corpus],
  { encoding: "utf8", maxBuffer: 1 << 28 },
);
if (dump.status !== 0) {
  console.error(dump.stderr);
  process.exit(1);
}
const target = JSON.parse(dump.stdout);
const verses = Object.keys(target).length;

const mod = await import("../pkg-web/sous_chef_web.js");
const wasmBytes = readFileSync(new URL("../pkg-web/sous_chef_web_bg.wasm", import.meta.url));
await mod.default({ module_or_path: wasmBytes });

// Warm-up, then measure.
const WARMUP = 2;
const RUNS = 10;
let findings = 0;
for (let i = 0; i < WARMUP; i++) mod.analyze_vref(target, undefined, undefined);
const times = [];
for (let i = 0; i < RUNS; i++) {
  const t0 = performance.now();
  const out = mod.analyze_vref(target, undefined, undefined);
  times.push(performance.now() - t0);
  findings = out.length;
}
times.sort((a, b) => a - b);
const median = times[Math.floor(times.length / 2)];

console.log(
  `${corpus}: ${verses} verses, ${findings} findings | ` +
    `wasm analyze_vref median ${median.toFixed(1)} ms ` +
    `(${((median * 1000) / verses).toFixed(1)} µs/verse), ` +
    `min ${times[0].toFixed(1)} ms over ${RUNS} runs`,
);
