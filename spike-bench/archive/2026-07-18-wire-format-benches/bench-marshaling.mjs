// Throwaway harness (scratchpad only, not part of the repo): measures the
// wasm→JS marshaling step in isolation, for the two competing designs:
//   (a) bench_synthetic_findings        -> Vec<Finding> marshaled to a JS
//       object array (tsify::into_wasm_abi), the same path analyze_vref uses.
//   (b) bench_synthetic_findings_packed -> Vec<u8> marshaled to a Uint8Array.
//
// Both functions synthesize their `count` findings without any corpus/rule
// compute (see crates/wasm/src/lib.rs, `bench-probes` feature), so whatever
// time is measured here is (approximately) pure marshaling cost — the
// missing middle piece between "Rust-side Finding allocation" and
// "postMessage/structured-clone of an already-existing JS value."
//
// Usage: node bench-marshaling.mjs

const mod = await import(
  "/Users/willkelly/Documents/Work/Code/scripture-sous-chef/.claude/worktrees/line-cook-finding-address/pkg-node/sous_chef_web_bench.js"
);

const COUNTS = [124, 240, 317, 415, 611, 5415];
const WARMUP = 5;
const TRIALS = 30;

function median(times) {
  const sorted = [...times].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[mid - 1] + sorted[mid]) / 2 : sorted[mid];
}

function timeCalls(fn, count) {
  for (let i = 0; i < WARMUP; i++) fn(count);
  const times = [];
  for (let i = 0; i < TRIALS; i++) {
    const t0 = performance.now();
    fn(count);
    times.push(performance.now() - t0);
  }
  return times;
}

const rows = [];
for (const count of COUNTS) {
  const objTimes = timeCalls(mod.bench_synthetic_findings, count);
  const packedTimes = timeCalls(mod.bench_synthetic_findings_packed, count);
  rows.push({
    count,
    objMedianMs: median(objTimes),
    objMinMs: Math.min(...objTimes),
    packedMedianMs: median(packedTimes),
    packedMinMs: Math.min(...packedTimes),
  });
}

console.log(
  "count".padStart(6),
  "| obj array median (ms)".padStart(24),
  "| obj min (ms)".padStart(15),
  "| packed buf median (ms)".padStart(25),
  "| packed min (ms)".padStart(18),
);
for (const r of rows) {
  console.log(
    String(r.count).padStart(6),
    r.objMedianMs.toFixed(4).padStart(24),
    r.objMinMs.toFixed(4).padStart(15),
    r.packedMedianMs.toFixed(4).padStart(25),
    r.packedMinMs.toFixed(4).padStart(18),
  );
}
