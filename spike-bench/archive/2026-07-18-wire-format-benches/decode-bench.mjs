// Measurement task: decode cost for the proposed packed-binary Finding
// wire format (see bench-binary.mjs), which bench-binary.mjs itself does not
// measure -- it only measures the postMessage clone/transfer cost of moving
// raw bytes across the worker boundary. On the object-array baseline
// (bench.mjs), a Finding is immediately usable the instant it's received:
// no decode step exists. On the packed-buffer side, the receiver only has
// raw bytes; something has to walk the buffer and turn each 16-byte record
// into the same shape bench.mjs's rowToFinding produces (sid/code/start/end)
// before it's usable. This script measures that decode step, both eagerly
// (decode every record on receipt) and lazily (decode exactly one record,
// as if only rendering a single on-screen finding).
//
// Percentile -> corpus -> count picks are lifted verbatim from
// bench-binary.mjs's own run (see its "Percentile -> nearest real corpus"
// output), so these numbers line up with the existing postMessage bench.
//
// Buffer layout matches bench-binary.mjs's BYTES_PER_RECORD = 16:
//   byte 0       : rule id (u8)      -> index into RULE_CODES
//   bytes 1-2    : key_idx (u16 LE)  -> index into VERSE_REFS
//   bytes 3-4    : span start (u16 LE)
//   bytes 5-6    : span end (u16 LE)
//   bytes 7-15   : unused / reserved
//
// bench-binary.mjs's makePackedBuffer() fills the buffer with an arbitrary
// Knuth-hash byte pattern, which is fine for a pure clone/transfer-cost
// bench (byte contents never inspected) but useless here: decode has to
// resolve rule id and key_idx through real lookup tables, so the buffer
// below is built the same way (flat ArrayBuffer, count*16 bytes, DataView
// writes in a loop) but with field values constrained to realistic,
// in-range indices instead of an opaque hash fill.

import { performance } from 'node:perf_hooks';

// ---------------- Realistic lookup tables ----------------

// Rule catalog: rule id (the 1-byte field) -> rule code string. A realistic
// sample of this codebase's rule catalog size (StatefulRule catalog runs to
// a few dozen entries, not hundreds) -- names patterned after this repo's
// actual rule-code convention (`<family>.<check-name>`).
const RULE_CODES = [
  'prop.length-ratio',
  'lex.duplicate-word',
  'punct.spacing-anomaly',
  'casing.sentence-initial',
  'casing.word-lexicon',
  'quote.unbalanced-pair',
  'quote.nested-depth',
  'terminal.missing-close',
  'terminal.strength-mismatch',
  'script.mixed-script',
  'number.digit-inconsistency',
  'whitespace.double-space',
  'whitespace.trailing',
  'punct.repeated-terminal',
  'lex.rare-token',
  'prop.token-count-outlier',
  'casing.all-caps-run',
  'punct.bracket-mismatch',
  'lex.transliteration-drift',
  'prop.verse-length-outlier',
];

// Verse-ref lookup: key_idx (the 2-byte field) -> "BOOK CH:VS" string. In the
// real architecture the consumer (editor) already holds this array, since it
// supplied the original VerseMap to the engine -- resolving key_idx is an
// array index, not a parse. Synthesize a few thousand entries spanning many
// books/chapters so key_idx values scattered across a corpus-sized buffer
// all resolve to something realistic.
const BOOKS = [
  'GEN', 'EXO', 'LEV', 'NUM', 'DEU', 'JOS', 'JDG', 'RUT', '1SA', '2SA',
  '1KI', '2KI', '1CH', '2CH', 'EZR', 'NEH', 'EST', 'JOB', 'PSA', 'PRO',
  'ECC', 'SNG', 'ISA', 'JER', 'LAM', 'EZK', 'DAN', 'HOS', 'JOL', 'AMO',
  'OBA', 'JON', 'MIC', 'NAM', 'HAB', 'ZEP', 'HAG', 'ZEC', 'MAL', 'MAT',
  'MRK', 'LUK', 'JHN', 'ACT', 'ROM', '1CO', '2CO', 'GAL', 'EPH', 'PHP',
  'COL', '1TH', '2TH', '1TI', '2TI', 'TIT', 'PHM', 'HEB', 'JAS', '1PE',
  '2PE', '1JN', '2JN', '3JN', 'JUD', 'REV',
];

function buildVerseRefs(target) {
  const refs = [];
  outer: for (const book of BOOKS) {
    for (let ch = 1; ch <= 40; ch++) {
      for (let vs = 1; vs <= 30; vs++) {
        refs.push(`${book} ${ch}:${vs}`);
        if (refs.length >= target) break outer;
      }
    }
  }
  return refs;
}

const VERSE_REFS = buildVerseRefs(8000);

const BYTES_PER_RECORD = 16;

// ---------------- Percentile picks (verbatim from bench-binary.mjs) ----------------

const PERCENTILE_PICKS = [
  { p: 1, corpus_id: 'WA-auh-reg', count: 124 },
  { p: 10, corpus_id: 'WA-knx-x-bajare-reg', count: 240 },
  { p: 25, corpus_id: 'WA-gnh-reg', count: 317 },
  { p: 50, corpus_id: 'WA-bds-reg', count: 415 },
  { p: 75, corpus_id: 'WA-lmn-x-anjara-reg', count: 611 },
  { p: 99, corpus_id: 'WA-as-ulb', count: 5415 },
];

// ---------------- Buffer construction ----------------
// Same shape as bench-binary.mjs's makePackedBuffer (flat ArrayBuffer,
// count * BYTES_PER_RECORD, populated in a single loop) but with per-field
// values instead of a raw hash fill, since decode requires resolvable
// indices. Values are realistic-looking, not byte-perfect-correct: rule id
// cycles through the whole catalog, key_idx is scattered across the verse
// lookup via a coprime stride (97 and 8000 share no factors, so it's a full
// permutation, not a short repeating cycle), spans are small in-verse
// offsets.
function makePackedBuffer(count) {
  const buf = new ArrayBuffer(count * BYTES_PER_RECORD);
  const view = new DataView(buf);
  for (let i = 0; i < count; i++) {
    const offset = i * BYTES_PER_RECORD;
    const ruleId = i % RULE_CODES.length;
    const keyIdx = (i * 97 + 13) % VERSE_REFS.length;
    const start = (i * 7) % 60;
    const end = start + 1 + (i % 25);
    view.setUint8(offset, ruleId);
    view.setUint16(offset + 1, keyIdx, true);
    view.setUint16(offset + 3, start, true);
    view.setUint16(offset + 5, end, true);
    // bytes 7..15 unused/reserved, left zero.
  }
  return buf;
}

// ---------------- Decode ----------------
// What a consumer actually needs to render a finding: the resolved rule
// code string, the resolved verse-ref string, and the span. Mirrors
// bench.mjs's rowToFinding output shape (sid/code/start/end).
function decodeRecord(view, i) {
  const offset = i * BYTES_PER_RECORD;
  const ruleId = view.getUint8(offset);
  const keyIdx = view.getUint16(offset + 1, true);
  const start = view.getUint16(offset + 3, true);
  const end = view.getUint16(offset + 5, true);
  return {
    code: RULE_CODES[ruleId],
    sid: VERSE_REFS[keyIdx],
    start,
    end,
  };
}

function decodeAll(buf, count) {
  const view = new DataView(buf);
  const out = new Array(count);
  for (let i = 0; i < count; i++) {
    out[i] = decodeRecord(view, i);
  }
  return out;
}

// ---------------- stats helpers (median/stddev, verbatim convention from
// bench.mjs / bench-binary.mjs) ----------------

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

// Module-level sink so V8 can't dead-code-eliminate the decode work as
// unused; same role as the ack round-trip in the postMessage benches.
let sink;

// Eager: decode every record in the buffer immediately upon "receipt" (the
// naive approach -- what you'd reach for first with a packed buffer).
//
// A single decodeAll() call at these percentiles' counts (124..5415 records,
// each record a handful of nanoseconds of work) finishes in well under a
// microsecond to a few tens of microseconds -- too close to
// performance.now()'s per-call noise floor (GC pauses, JIT tiering, timer
// jitter) to time reliably one call at a time; an initial pass at that
// (single t0/t1 per trial) produced non-monotonic, high-stddev results that
// didn't scale sanely with count. Fixed by the same amortization trick used
// for the single-record case below: each trial runs decodeAll() back-to-back
// INNER_ITERS_EAGER times (scaled so every percentile does roughly the same
// total record-decode work per trial, keeping the noise floor comparable
// across rows) and reports the per-call time.
const TARGET_RECORD_WORK_PER_TRIAL = 200_000;

function benchEagerDecodeAll(buf, count, { warmup = 5, trials = 30 } = {}) {
  const innerIters = Math.max(3, Math.round(TARGET_RECORD_WORK_PER_TRIAL / count));

  for (let i = 0; i < warmup; i++) {
    for (let j = 0; j < innerIters; j++) sink = decodeAll(buf, count);
  }

  const times = [];
  for (let i = 0; i < trials; i++) {
    const t0 = performance.now();
    for (let j = 0; j < innerIters; j++) sink = decodeAll(buf, count);
    const t1 = performance.now();
    times.push((t1 - t0) / innerIters);
  }
  return { medianMs: median(times), times, innerIters };
}

// Lazy/single: decode exactly one record, simulating "only decode the
// specific finding actually being rendered right now." A single call is far
// too fast for performance.now()'s resolution to time reliably on its own,
// so each trial times a tight inner loop of INNER_ITERS single-record
// decodes and reports the amortized per-call cost -- standard microbenchmark
// practice for sub-microsecond operations. This does not change what's
// measured: every iteration is still exactly one 16-byte record -> one
// resolved object, and the result is independent of the buffer's total
// record count (it's a fixed-offset read + two array indexes, O(1)
// regardless of how many other records exist in the buffer).
const INNER_ITERS = 200_000;

function benchLazyDecodeOne(buf, count, { warmup = 5, trials = 30 } = {}) {
  const view = new DataView(buf);
  const idx = Math.floor(count / 2); // an arbitrary "currently rendered" record

  for (let i = 0; i < warmup; i++) {
    for (let j = 0; j < INNER_ITERS; j++) sink = decodeRecord(view, idx);
  }

  const perCallUsTimes = [];
  for (let i = 0; i < trials; i++) {
    const t0 = performance.now();
    for (let j = 0; j < INNER_ITERS; j++) sink = decodeRecord(view, idx);
    const t1 = performance.now();
    perCallUsTimes.push(((t1 - t0) * 1000) / INNER_ITERS);
  }
  return { medianUs: median(perCallUsTimes), times: perCallUsTimes };
}

// ---------------- main ----------------

function main() {
  console.log(`Rule catalog size: ${RULE_CODES.length}`);
  console.log(`Verse-ref lookup size: ${VERSE_REFS.length}`);
  console.log('');

  const rows = [];
  for (const { p, corpus_id, count } of PERCENTILE_PICKS) {
    const buf = makePackedBuffer(count);

    // Sanity check: decode a record and confirm it resolved to real strings,
    // not undefined -- catches an off-by-range lookup table before it shows
    // up as a silently-too-fast benchmark number.
    const view = new DataView(buf);
    const sample = decodeRecord(view, 0);
    if (!sample.code || !sample.sid) {
      throw new Error(`decode sanity check failed for ${corpus_id}: ${JSON.stringify(sample)}`);
    }

    const eager = benchEagerDecodeAll(buf, count);
    const lazy = benchLazyDecodeOne(buf, count);
    const impliedPerFindingUs = (eager.medianMs * 1000) / count;

    rows.push({
      p,
      corpus_id,
      count,
      eagerMedianMs: eager.medianMs,
      eagerSdMs: stddev(eager.times, eager.medianMs),
      lazyMedianUs: lazy.medianUs,
      lazySdUs: stddev(lazy.times, lazy.medianUs),
      impliedPerFindingUs,
    });

    console.log(
      `p${p} (${corpus_id}, n=${count}, innerIters=${eager.innerIters}): eager-all=${eager.medianMs.toFixed(4)}ms ` +
        `(sd=${stddev(eager.times, eager.medianMs).toFixed(4)}), ` +
        `single-decode=${lazy.medianUs.toFixed(4)}us ` +
        `(sd=${stddev(lazy.times, lazy.medianUs).toFixed(4)}), ` +
        `implied-per-finding=${impliedPerFindingUs.toFixed(4)}us`
    );
  }

  console.log('\n=== FINAL TABLE JSON ===');
  console.log(JSON.stringify(rows, null, 2));

  console.log('\n=== FINAL TABLE (markdown) ===');
  console.log(
    '| percentile | corpus | count | eager-decode-all median (ms) | single-finding decode (us) | implied per-finding from eager (us) |'
  );
  console.log('|---|---|---|---|---|---|');
  for (const r of rows) {
    console.log(
      `| p${r.p} | ${r.corpus_id} | ${r.count} | ${r.eagerMedianMs.toFixed(4)} | ${r.lazyMedianUs.toFixed(4)} | ${r.impliedPerFindingUs.toFixed(4)} |`
    );
  }
}

main();
