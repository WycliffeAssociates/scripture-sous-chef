// Extends bench.mjs: tests the "pack each Finding into a fixed 16-byte binary
// record, ship the whole corpus as one flat ArrayBuffer" proposal against the
// same percentile finding-counts already identified by bench.mjs for the
// all-config (primary) scenario, over:
//   /tmp/oracle-casing-fx/after.all.wa.tsv
//
// The TSV-parsing / percentile / nearest-real-corpus selection logic below is
// copied verbatim from bench.mjs so the picked corpora and counts are
// byte-for-byte identical, making these numbers directly comparable to the
// existing JS-object-array postMessage baseline recorded there.
//
// Three scenarios measured per percentile via worker_threads postMessage:
//   (a) baseline, cited from bench.mjs (JS object array, structured clone)
//   (b) packed ArrayBuffer, default clone (no transfer list)
//   (c) packed ArrayBuffer, transferred (postMessage(buf, [buf]))
//
// Plain (non-shared) ArrayBuffer throughout — SharedArrayBuffer is a
// different feature (concurrent shared memory, needs COOP/COEP in a real
// browser) and isn't needed for this one-directional handoff test.

import { readFileSync } from 'node:fs';
import { Worker } from 'node:worker_threads';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { performance } from 'node:perf_hooks';

const __dirname = dirname(fileURLToPath(import.meta.url));

const FILES = {
  all: '/tmp/oracle-casing-fx/after.all.wa.tsv',
};

const PERCENTILES = [1, 10, 25, 50, 75, 99];

const BYTES_PER_RECORD = 16; // u128: rule id + verse key idx + span start/end + free bits

// ---------- TSV parsing (verbatim from bench.mjs) ----------

function parseTsv(path) {
  const text = readFileSync(path, 'utf8');
  const lines = text.split('\n').filter((l) => l.length > 0);
  const rows = [];
  for (const line of lines) {
    const cols = line.split('\t');
    if (cols.length !== 9) {
      throw new Error(`expected 9 columns, got ${cols.length}: ${line.slice(0, 80)}`);
    }
    const [corpus_id, scope, verse_ref, rule_code, start, end, severity, score, args] = cols;
    rows.push({
      corpus_id,
      scope,
      verse_ref,
      rule_code,
      start: Number(start),
      end: Number(end),
      severity,
      score: score === '-' ? null : Number(score),
      args: args === '-' ? null : JSON.parse(args),
    });
  }
  return rows;
}

function groupByCorpus(rows) {
  const map = new Map();
  for (const row of rows) {
    if (!map.has(row.corpus_id)) map.set(row.corpus_id, []);
    map.get(row.corpus_id).push(row);
  }
  return map;
}

// ---------- stats (verbatim from bench.mjs) ----------

function percentileLinear(sortedAsc, p) {
  const n = sortedAsc.length;
  if (n === 1) return sortedAsc[0];
  const rank = (p / 100) * (n - 1);
  const lo = Math.floor(rank);
  const hi = Math.ceil(rank);
  if (lo === hi) return sortedAsc[lo];
  const frac = rank - lo;
  return sortedAsc[lo] + (sortedAsc[hi] - sortedAsc[lo]) * frac;
}

function distributionStats(counts) {
  const sorted = [...counts].sort((a, b) => a - b);
  const n = sorted.length;
  const mean = sorted.reduce((a, b) => a + b, 0) / n;
  const stats = { n, min: sorted[0], max: sorted[n - 1], mean };
  for (const p of PERCENTILES) {
    stats[`p${p}`] = percentileLinear(sorted, p);
  }
  return stats;
}

function nearestCorpus(countsByCorpus, target) {
  let best = null;
  let bestDiff = Infinity;
  for (const [corpus_id, count] of countsByCorpus) {
    const diff = Math.abs(count - target);
    if (diff < bestDiff || (diff === bestDiff && corpus_id < best.corpus_id)) {
      best = { corpus_id, count };
      bestDiff = diff;
    }
  }
  return best;
}

// ---------- benchmarking ----------

function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  const n = s.length;
  const mid = Math.floor(n / 2);
  return n % 2 === 0 ? (s[mid - 1] + s[mid]) / 2 : s[mid];
}

function stddev(arr, mean) {
  const v = arr.reduce((a, b) => a + (b - mean) ** 2, 0) / arr.length;
  return Math.sqrt(v);
}

// Build a packed ArrayBuffer of `n` 16-byte records, filled with an
// arbitrary (non-zero, non-degenerate) byte pattern so it isn't a trivial
// all-zeros page.
function makePackedBuffer(n) {
  const buf = new ArrayBuffer(n * BYTES_PER_RECORD);
  const view = new Uint8Array(buf);
  for (let i = 0; i < view.length; i++) {
    view[i] = (i * 2654435761) & 0xff; // Knuth multiplicative hash, cheap & non-zero
  }
  return buf;
}

function makeRoundTripperBuffer(worker) {
  return function roundTrip(buf, transfer) {
    return new Promise((resolve, reject) => {
      const t0 = performance.now();
      const onMsg = (msg) => {
        if (msg && msg.type === 'ack') {
          const t1 = performance.now();
          worker.off('message', onMsg);
          worker.off('error', onErr);
          resolve(t1 - t0);
        }
      };
      const onErr = (err) => {
        worker.off('message', onMsg);
        worker.off('error', onErr);
        reject(err);
      };
      worker.on('message', onMsg);
      worker.on('error', onErr);
      if (transfer) {
        worker.postMessage({ type: 'buffer', payload: buf }, [buf]);
      } else {
        worker.postMessage({ type: 'buffer', payload: buf });
      }
    });
  };
}

// Fresh buffer built per trial (not just per warmup) — required for the
// transfer case since a transferred ArrayBuffer is detached on the sending
// side afterward and cannot be reused; applied uniformly to the clone case
// too so both scenarios do identical per-trial work outside the timed
// interval (buffer construction happens before t0 either way).
async function benchPostMessageBuffer(worker, n, { transfer, warmup = 5, trials = 30 }) {
  const roundTrip = makeRoundTripperBuffer(worker);

  for (let i = 0; i < warmup; i++) {
    const buf = makePackedBuffer(n);
    await roundTrip(buf, transfer);
  }

  // Sanity check once: confirm transfer actually detaches (byteLength -> 0)
  // and clone does not, so we know the two scenarios are exercising the
  // semantics we think they are.
  {
    const buf = makePackedBuffer(n);
    await roundTrip(buf, transfer);
    if (transfer) {
      if (buf.byteLength !== 0) {
        throw new Error(`expected transferred buffer to be detached (byteLength 0), got ${buf.byteLength}`);
      }
    } else {
      if (buf.byteLength !== n * BYTES_PER_RECORD) {
        throw new Error(`expected cloned buffer to remain intact, got byteLength ${buf.byteLength}`);
      }
    }
  }

  const times = [];
  for (let i = 0; i < trials; i++) {
    const buf = makePackedBuffer(n);
    times.push(await roundTrip(buf, transfer));
  }
  return { medianMs: median(times), times };
}

// ---------- main ----------

async function main() {
  const allRows = parseTsv(FILES.all);
  const allByCorpus = groupByCorpus(allRows);
  const allCounts = [...allByCorpus.values()].map((r) => r.length);
  const allStats = distributionStats(allCounts);
  const allCountMap = new Map([...allByCorpus].map(([k, v]) => [k, v.length]));

  console.log('=== Percentile -> nearest real corpus (all-config, primary) [should match bench.mjs] ===');
  const allPicks = PERCENTILES.map((p) => {
    const target = allStats[`p${p}`];
    const picked = nearestCorpus(allCountMap, target);
    console.log(`p${p}: target=${target.toFixed(1)} -> ${picked.corpus_id} (count=${picked.count})`);
    return { p, target, ...picked };
  });

  const worker = new Worker(join(__dirname, 'worker-binary.mjs'));
  await new Promise((resolve, reject) => {
    worker.once('online', resolve);
    worker.once('error', reject);
  });

  console.log('\n=== Benchmarking packed ArrayBuffer postMessage (clone, no transfer list) ===');
  const cloneResults = [];
  for (const { p, corpus_id, count } of allPicks) {
    const res = await benchPostMessageBuffer(worker, count, { transfer: false });
    const jitter = stddev(res.times, res.medianMs);
    cloneResults.push({ p, corpus_id, count, medianMs: res.medianMs, stddevMs: jitter });
    console.log(
      `p${p} (${corpus_id}, n=${count}, bytes=${count * BYTES_PER_RECORD}): clone=${res.medianMs.toFixed(4)}ms (sd=${jitter.toFixed(4)})`
    );
  }

  console.log('\n=== Benchmarking packed ArrayBuffer postMessage (transferred) ===');
  const transferResults = [];
  for (const { p, corpus_id, count } of allPicks) {
    const res = await benchPostMessageBuffer(worker, count, { transfer: true });
    const jitter = stddev(res.times, res.medianMs);
    transferResults.push({ p, corpus_id, count, medianMs: res.medianMs, stddevMs: jitter });
    console.log(
      `p${p} (${corpus_id}, n=${count}, bytes=${count * BYTES_PER_RECORD}): transfer=${res.medianMs.toFixed(4)}ms (sd=${jitter.toFixed(4)})`
    );
  }

  console.log('\n=== FINAL TABLE (packed ArrayBuffer, clone) JSON ===');
  console.log(JSON.stringify(cloneResults, null, 2));
  console.log('\n=== FINAL TABLE (packed ArrayBuffer, transferred) JSON ===');
  console.log(JSON.stringify(transferResults, null, 2));

  await worker.terminate();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
