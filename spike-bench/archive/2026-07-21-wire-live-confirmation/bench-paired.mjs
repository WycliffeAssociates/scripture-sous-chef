// Paired A/B of total analyze_vref wall time: dev (object array wire) vs
// packed (Uint8Array wire), both wasm modules loaded in ONE process,
// trials interleaved (order swapped every pair) so machine-load drift
// cancels in the per-pair delta. Compute is oracle-proven identical, so
// median(per-pair delta) is the wire-stage saving.

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';

const require = createRequire(import.meta.url);
const SCRATCH = '/private/tmp/claude-503/-Users-willkelly-Documents-Work-Code-scripture-sous-chef/fbf8f847-a71c-4108-aeec-39a55d544cdc/scratchpad';
const dev = require(join(SCRATCH, 'pkg-node-dev', 'ssc_wasm.js'));
const packed = require(join(SCRATCH, 'pkg-node-packed', 'ssc_wasm.js'));

const CORPORA_DIR = '/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref';
const CORPORA = ['WA-bds-reg', 'WA-as-ulb'];

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

function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 === 0 ? (s[mid - 1] + s[mid]) / 2 : s[mid];
}

let sink;

function timeCall(mod, corpus) {
  const t0 = performance.now();
  sink = mod.analyze_vref({ keys: corpus.keys, texts: corpus.texts }, null, ALL_ON);
  const t1 = performance.now();
  return t1 - t0;
}

const WARMUP = 3;
const PAIRS = 40;

for (const id of CORPORA) {
  const corpus = loadVref(id);
  for (let i = 0; i < WARMUP; i++) {
    timeCall(dev, corpus);
    timeCall(packed, corpus);
  }
  const devTimes = [];
  const packedTimes = [];
  const deltas = [];
  for (let i = 0; i < PAIRS; i++) {
    let d, p;
    if (i % 2 === 0) {
      d = timeCall(dev, corpus);
      p = timeCall(packed, corpus);
    } else {
      p = timeCall(packed, corpus);
      d = timeCall(dev, corpus);
    }
    devTimes.push(d);
    packedTimes.push(p);
    deltas.push(d - p);
  }
  console.log(JSON.stringify({
    corpus: id,
    pairs: PAIRS,
    devMedian: median(devTimes),
    packedMedian: median(packedTimes),
    deltaMedian: median(deltas),
    deltaMin: Math.min(...deltas),
    deltaMax: Math.max(...deltas),
    deltas: deltas.map((x) => +x.toFixed(2)),
  }, null, 2));
}
