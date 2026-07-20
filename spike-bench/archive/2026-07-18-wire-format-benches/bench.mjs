// Measurement task: per-corpus finding-count distribution over the real
// oracle dumps, plus JSON.stringify and worker_threads postMessage
// (structured-clone) timings for the Finding[] payload at each percentile.
//
// No production code is touched. Pure analysis over:
//   /tmp/oracle-casing-fx/after.all.wa.tsv      (Config::all())
//   /tmp/oracle-casing-fx/after.default.wa.tsv  (Config::v1_defaults())

import { readFileSync } from 'node:fs';
import { Worker } from 'node:worker_threads';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { performance } from 'node:perf_hooks';

const __dirname = dirname(fileURLToPath(import.meta.url));

const FILES = {
  all: '/tmp/oracle-casing-fx/after.all.wa.tsv',
  default: '/tmp/oracle-casing-fx/after.default.wa.tsv',
};

const PERCENTILES = [1, 10, 25, 50, 75, 99];

// ---------- TSV parsing ----------

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

function rowToFinding(row) {
  const finding = {
    sid: row.verse_ref,
    code: row.rule_code,
    severity: row.severity,
    start: row.start,
    end: row.end,
  };
  if (row.score !== null) finding.score = row.score;
  if (row.args !== null) finding.args = row.args;
  return finding;
}

function groupByCorpus(rows) {
  const map = new Map();
  for (const row of rows) {
    if (!map.has(row.corpus_id)) map.set(row.corpus_id, []);
    map.get(row.corpus_id).push(row);
  }
  return map;
}

// ---------- stats ----------

function percentileLinear(sortedAsc, p) {
  // numpy-style linear interpolation percentile, p in [0,100]
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

// pick the real corpus whose count is closest to `target`; ties broken by
// picking the smallest corpus_id string for determinism.
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

function benchStringify(findings, warmup = 5, trials = 30) {
  let lastStr = '';
  for (let i = 0; i < warmup; i++) lastStr = JSON.stringify(findings);
  const times = [];
  for (let i = 0; i < trials; i++) {
    const t0 = performance.now();
    lastStr = JSON.stringify(findings);
    const t1 = performance.now();
    times.push(t1 - t0);
  }
  const bytes = Buffer.byteLength(lastStr, 'utf8');
  return { medianMs: median(times), times, bytes };
}

function makeRoundTripper(worker) {
  return function roundTrip(findings) {
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
      worker.postMessage({ type: 'findings', payload: findings });
    });
  };
}

async function benchPostMessage(worker, findings, warmup = 5, trials = 30) {
  const roundTrip = makeRoundTripper(worker);
  for (let i = 0; i < warmup; i++) await roundTrip(findings);
  const times = [];
  for (let i = 0; i < trials; i++) {
    times.push(await roundTrip(findings));
  }
  return { medianMs: median(times), times };
}

function stddev(arr, mean) {
  const v = arr.reduce((a, b) => a + (b - mean) ** 2, 0) / arr.length;
  return Math.sqrt(v);
}

// ---------- main ----------

async function main() {
  const allRows = parseTsv(FILES.all);
  const defaultRows = parseTsv(FILES.default);

  const allByCorpus = groupByCorpus(allRows);
  const defaultByCorpus = groupByCorpus(defaultRows);

  const allCounts = [...allByCorpus.values()].map((r) => r.length);
  const defaultCounts = [...defaultByCorpus.values()].map((r) => r.length);

  const allStats = distributionStats(allCounts);
  const defaultStats = distributionStats(defaultCounts);

  console.log('=== Per-corpus finding-count distribution ===');
  console.log('all (Config::all(), noisy/high-aggression):');
  console.log(JSON.stringify(allStats, null, 2));
  console.log('default (Config::v1_defaults(), shipped default):');
  console.log(JSON.stringify(defaultStats, null, 2));

  // corpus_id -> count maps for nearest-corpus lookup
  const allCountMap = new Map([...allByCorpus].map(([k, v]) => [k, v.length]));
  const defaultCountMap = new Map([...defaultByCorpus].map(([k, v]) => [k, v.length]));

  console.log('\n=== Percentile -> nearest real corpus (all-config, primary) ===');
  const allPicks = PERCENTILES.map((p) => {
    const target = allStats[`p${p}`];
    const picked = nearestCorpus(allCountMap, target);
    console.log(`p${p}: target=${target.toFixed(1)} -> ${picked.corpus_id} (count=${picked.count})`);
    return { p, target, ...picked };
  });

  console.log('\n=== Percentile -> nearest real corpus (default-config, contrast) ===');
  const defaultPicks = PERCENTILES.map((p) => {
    const target = defaultStats[`p${p}`];
    const picked = nearestCorpus(defaultCountMap, target);
    console.log(`p${p}: target=${target.toFixed(1)} -> ${picked.corpus_id} (count=${picked.count})`);
    return { p, target, ...picked };
  });

  // Build wire-shaped Finding[] arrays for the primary (all-config) picks.
  const allFindingArrays = allPicks.map(({ p, corpus_id, count }) => {
    const rows = allByCorpus.get(corpus_id);
    return { p, corpus_id, count, findings: rows.map(rowToFinding) };
  });

  // And for the default-config picks (contrast).
  const defaultFindingArrays = defaultPicks.map(({ p, corpus_id, count }) => {
    const rows = defaultByCorpus.get(corpus_id);
    return { p, corpus_id, count, findings: rows.map(rowToFinding) };
  });

  // spin up one persistent worker for all postMessage benchmarks
  const worker = new Worker(join(__dirname, 'worker.mjs'));
  // make sure it's alive
  await new Promise((resolve, reject) => {
    worker.once('online', resolve);
    worker.once('error', reject);
  });

  console.log('\n=== Benchmarking (primary: all-config) ===');
  const primaryResults = [];
  for (const { p, corpus_id, count, findings } of allFindingArrays) {
    const strRes = benchStringify(findings);
    const pmRes = await benchPostMessage(worker, findings);
    const jitter = stddev(strRes.times, strRes.medianMs);
    const pmJitter = stddev(pmRes.times, pmRes.medianMs);
    primaryResults.push({
      p,
      corpus_id,
      count,
      stringifyMedianMs: strRes.medianMs,
      stringifyStddevMs: jitter,
      bytes: strRes.bytes,
      postMessageMedianMs: pmRes.medianMs,
      postMessageStddevMs: pmJitter,
    });
    console.log(
      `p${p} (${corpus_id}, n=${count}): stringify=${strRes.medianMs.toFixed(3)}ms (sd=${jitter.toFixed(3)}) ` +
        `bytes=${strRes.bytes} postMessage=${pmRes.medianMs.toFixed(3)}ms (sd=${pmJitter.toFixed(3)})`
    );
  }

  console.log('\n=== Benchmarking (contrast: default-config) ===');
  const contrastResults = [];
  for (const { p, corpus_id, count, findings } of defaultFindingArrays) {
    const strRes = benchStringify(findings);
    const pmRes = await benchPostMessage(worker, findings);
    contrastResults.push({
      p,
      corpus_id,
      count,
      stringifyMedianMs: strRes.medianMs,
      bytes: strRes.bytes,
      postMessageMedianMs: pmRes.medianMs,
    });
    console.log(
      `p${p} (${corpus_id}, n=${count}): stringify=${strRes.medianMs.toFixed(3)}ms bytes=${strRes.bytes} ` +
        `postMessage=${pmRes.medianMs.toFixed(3)}ms`
    );
  }

  console.log('\n=== FINAL TABLE (primary, all-config) JSON ===');
  console.log(JSON.stringify(primaryResults, null, 2));
  console.log('\n=== CONTRAST TABLE (default-config) JSON ===');
  console.log(JSON.stringify(contrastResults, null, 2));

  await worker.terminate();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
