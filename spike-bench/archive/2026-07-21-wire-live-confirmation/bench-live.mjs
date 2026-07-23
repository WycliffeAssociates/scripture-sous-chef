// Post-implementation before/after of the findings wire cost on the two LIVE
// analyze_vref surfaces (dev = JS object array, packed = Uint8Array wire).
// Usage: node bench-live.mjs <pkg-dir> <object|packed>
//
// Measures, per corpus (WA-bds-reg p50, WA-as-ulb p99), all rules enabled:
//   a. total analyze_vref call (compute + wire construction + boundary)
//   b. postMessage to a worker_threads Worker (clone for object, transfer
//      for packed)
//   c. receive-side readiness (packed only: eager DataView decode of every
//      record; object arrays arrive ready — cost 0 by construction)

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { Worker } from 'node:worker_threads';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { performance } from 'node:perf_hooks';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

const [pkgDir, mode] = process.argv.slice(2);
if (!pkgDir || !['object', 'packed'].includes(mode)) {
  console.error('usage: node bench-live.mjs <pkg-dir> <object|packed>');
  process.exit(1);
}
const wasm = require(join(pkgDir, 'ssc_wasm.js'));

const CORPORA_DIR = '/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref';
const CORPORA = ['WA-bds-reg', 'WA-as-ulb'];

// The 25 RuleId strings (packed-findings-wire plan §2 / RuleId union in the
// generated .d.ts — verified identical in both builds).
const RULE_IDS = [
  'lex.excess-h-whitespace', 'hyg.tab-in-body', 'hyg.control-chars',
  'hyg.zero-width-misuse', 'hyg.empty-verse', 'hyg.invalid-codepoint',
  'hyg.replacement-run', 'prop.length-ratio', 'struct.source-marker-leftover',
  'struct.merge-conflict-marker', 'punct.adjacency-anomaly',
  'lex.duplicate-word', 'lex.punct-only-token',
  'uni.combining-mark-without-base', 'uni.redundant-zero-width-space',
  'uni.mixed-script-in-token', 'lex.repeated-character-run',
  'uni.mixed-numeral-systems', 'punct.bracket-balance',
  'punct.spacing-anomaly', 'case.sentence-initial-lowercase',
  'case.inconsistent-word-casing', 'uni.rare-glyph', 'case.mixed-case-word',
  'uni.mixed-normalization',
];
const ALL_ON = { rules: Object.fromEntries(RULE_IDS.map((r) => [r, true])) };

// ---------- vref parsing (mirrors crates/core/dev/vref_io.rs) ----------

function loadVref(id) {
  const text = readFileSync(join(CORPORA_DIR, `${id}.txt`), 'utf8');
  const keys = [];
  const texts = [];
  for (const line of text.split('\n')) {
    const tab = line.indexOf('\t');
    if (tab === -1) continue;
    const key = line.slice(0, tab);
    const verse = line.slice(tab + 1);
    if (verse === '<range>') continue;
    keys.push(key);
    texts.push(verse);
  }
  return { keys, texts };
}

// ---------- stats ----------

function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 === 0 ? (s[mid - 1] + s[mid]) / 2 : s[mid];
}
const summarize = (times) => ({
  median: median(times),
  min: Math.min(...times),
  max: Math.max(...times),
});

// ---------- packed decode (ADR 0065 layout) ----------

const HEADER_LEN = 16;
const RECORD_LEN = 16;
const FLAG_HAS_SCORE = 1 << 2;

function packedCount(bytes) {
  if (bytes[0] !== 0x53 || bytes[1] !== 0x53 || bytes[2] !== 0x43 || bytes[3] !== 0x46) {
    throw new Error('bad magic');
  }
  if (bytes[4] !== 1 || bytes[5] !== RECORD_LEN) throw new Error('bad version/record_len');
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8, true);
}

function decodeAll(bytes, ruleTable) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const count = view.getUint32(8, true);
  const out = new Array(count);
  for (let i = 0; i < count; i++) {
    const o = HEADER_LEN + i * RECORD_LEN;
    const flags = view.getUint8(o + 1);
    out[i] = {
      code: ruleTable[view.getUint8(o)],
      key_idx: view.getUint32(o + 2, true),
      start: view.getUint16(o + 6, true),
      end: view.getUint16(o + 8, true),
      score: flags & FLAG_HAS_SCORE ? view.getUint16(o + 10, true) / 65535 : null,
    };
  }
  return out;
}

// ---------- postMessage round-trip ----------

function makeRoundTripper(worker) {
  return (msg, transferList) =>
    new Promise((resolve, reject) => {
      const t0 = performance.now();
      const onMsg = (m) => {
        if (m && m.type === 'ack') {
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
      if (transferList) worker.postMessage(msg, transferList);
      else worker.postMessage(msg);
    });
}

// ---------- main ----------

const ANALYZE_WARMUP = 3;
const ANALYZE_TRIALS = 20;
const PM_WARMUP = 3;
const PM_TRIALS = 30;
const DECODE_WARMUP = 3;
const DECODE_TRIALS = 30;
const DECODE_TARGET_RECORDS_PER_TRIAL = 200_000;

let sink; // defeat DCE

async function main() {
  const ruleTable = mode === 'packed' ? wasm.wire_rule_table() : null;

  const worker = new Worker(join(__dirname, 'worker-live.mjs'));
  await new Promise((resolve, reject) => {
    worker.once('online', resolve);
    worker.once('error', reject);
  });
  const roundTrip = makeRoundTripper(worker);

  const results = [];
  for (const id of CORPORA) {
    const corpus = loadVref(id);
    console.error(`[${mode}] ${id}: ${corpus.keys.length} verses`);

    // --- a. total analyze_vref ---
    let last;
    for (let i = 0; i < ANALYZE_WARMUP; i++) {
      last = wasm.analyze_vref({ keys: corpus.keys, texts: corpus.texts }, null, ALL_ON);
    }
    const analyzeTimes = [];
    for (let i = 0; i < ANALYZE_TRIALS; i++) {
      const t0 = performance.now();
      last = wasm.analyze_vref({ keys: corpus.keys, texts: corpus.texts }, null, ALL_ON);
      const t1 = performance.now();
      analyzeTimes.push(t1 - t0);
    }
    sink = last;

    const findingCount = mode === 'object' ? last.length : packedCount(last);
    const bufferBytes = mode === 'object' ? null : last.byteLength;

    // --- b. postMessage ---
    const pmTimes = [];
    if (mode === 'object') {
      for (let i = 0; i < PM_WARMUP; i++) await roundTrip({ type: 'findings', payload: last });
      for (let i = 0; i < PM_TRIALS; i++) {
        pmTimes.push(await roundTrip({ type: 'findings', payload: last }));
      }
    } else {
      // Transfer detaches the buffer, so send a fresh copy each trial; the
      // copy (slice) happens before t0 inside roundTrip's caller, untimed.
      for (let i = 0; i < PM_WARMUP; i++) {
        const bytes = last.slice();
        await roundTrip({ type: 'bytes', payload: bytes }, [bytes.buffer]);
      }
      {
        // sanity: transfer must detach
        const bytes = last.slice();
        await roundTrip({ type: 'bytes', payload: bytes }, [bytes.buffer]);
        if (bytes.buffer.byteLength !== 0) throw new Error('transfer did not detach');
      }
      for (let i = 0; i < PM_TRIALS; i++) {
        const bytes = last.slice();
        pmTimes.push(await roundTrip({ type: 'bytes', payload: bytes }, [bytes.buffer]));
      }
    }

    // --- c. receive-side readiness (packed only) ---
    let decode = null;
    let decodeInnerIters = null;
    if (mode === 'packed') {
      // A single decode of a few hundred records sits at performance.now()'s
      // noise floor, so amortize over an inner loop (same trick as the
      // 2026-07-18 spike's decode-bench.mjs) and report per-call time.
      decodeInnerIters = Math.max(3, Math.round(DECODE_TARGET_RECORDS_PER_TRIAL / findingCount));
      const sample = decodeAll(last, ruleTable);
      if (!sample[0] || !sample[0].code) throw new Error('decode sanity failed');
      for (let i = 0; i < DECODE_WARMUP; i++) {
        for (let j = 0; j < decodeInnerIters; j++) sink = decodeAll(last, ruleTable);
      }
      const decodeTimes = [];
      for (let i = 0; i < DECODE_TRIALS; i++) {
        const t0 = performance.now();
        for (let j = 0; j < decodeInnerIters; j++) sink = decodeAll(last, ruleTable);
        const t1 = performance.now();
        decodeTimes.push((t1 - t0) / decodeInnerIters);
      }
      decode = summarize(decodeTimes);
    }

    results.push({
      corpus: id,
      mode,
      verses: corpus.keys.length,
      findingCount,
      bufferBytes,
      analyze: summarize(analyzeTimes),
      postMessage: summarize(pmTimes),
      decode,
      decodeInnerIters,
    });
  }

  await worker.terminate();
  console.log(JSON.stringify(results, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
