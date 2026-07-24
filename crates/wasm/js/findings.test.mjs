// Node tests for the official JS wire surface (granularity-spine Appendix A
// §A.5, the cases exercisable before the wasm cutover). Run with:
//   node --test crates/wasm/js/findings.test.mjs
//
// Two kinds of coverage:
//  - cross-language parity: decode the Rust-encoder vectors (__vectors__.json,
//    emitted by `cargo xtask wire-vectors`) and assert the JS decoder produces
//    the Rust decoder's values, and rejects the same malformed categories;
//  - decode/reconcile/persistence behavior, driven by a test-local encoder that
//    is itself built from the generated schema (no hand-copied constants).

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { decodeFindings, decodePersistedFindings, reconcileFindings } from "./findings.js";
import {
  HEADER,
  SEVERITIES,
  RULE_TO_CODE,
  CODE_TO_RULE,
  CODE_TO_DIGEST,
  CODE_TO_INPUT_DEPENDENCY,
} from "./findings.generated.js";

const { RECORD_LEN, HEADER_LEN, OFFSETS, FLAGS } = HEADER;

// ---- test-local encoder (mirrors ssc-wire::pack, schema-driven) ----------

// spec: { analysisId: bigint, targetContextId: bigint, hasReference: bool,
//         records: [{ rule, severity, keyIdx, start, end, score?, hasArgs?,
//                     digest? }] }
function encode(spec) {
  const count = spec.records.length;
  const buf = new Uint8Array(HEADER_LEN + count * RECORD_LEN);
  const view = new DataView(buf.buffer);
  buf.set([...HEADER.MAGIC].map((c) => c.charCodeAt(0)), 0);
  view.setUint8(4, HEADER.VERSION);
  view.setUint8(5, RECORD_LEN);
  view.setUint8(6, HEADER_LEN);
  view.setUint8(OFFSETS.headerFlags, spec.hasReference ? FLAGS.headerHasReference : 0);
  view.setUint32(OFFSETS.count, count, true);
  view.setBigUint64(OFFSETS.targetContextId, spec.targetContextId, true);
  view.setBigUint64(OFFSETS.analysisId, spec.analysisId, true);

  spec.records.forEach((r, i) => {
    const base = HEADER_LEN + i * RECORD_LEN;
    const code = RULE_TO_CODE[r.rule];
    let flags = SEVERITIES.indexOf(r.severity);
    const hasScore = r.score !== undefined && r.score !== null;
    if (hasScore) flags |= FLAGS.hasScore;
    if (r.hasArgs) flags |= FLAGS.hasArgs;
    const digest = r.digest ?? { shape: "none" };
    if (digest.shape === "count-pair" && digest.saturated) flags |= FLAGS.payloadSaturated;
    view.setUint8(base + OFFSETS.recordCode, code);
    view.setUint8(base + OFFSETS.recordFlags, flags);
    view.setUint32(base + OFFSETS.recordKeyIdx, r.keyIdx, true);
    view.setUint16(base + OFFSETS.recordStart, r.start, true);
    view.setUint16(base + OFFSETS.recordEnd, r.end, true);
    view.setUint16(base + OFFSETS.recordScore, hasScore ? Math.round(r.score * 65535) : 0, true);
    const off = base + OFFSETS.recordPayload;
    if (digest.shape === "count-pair") {
      view.setUint16(off, digest.a, true);
      view.setUint16(off + 2, digest.b, true);
    } else if (digest.shape === "u32") {
      view.setUint32(off, digest.value, true);
    }
  });
  return buf;
}

function hexToBytes(hex) {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

const VECTORS = JSON.parse(
  readFileSync(fileURLToPath(new URL("./__vectors__.json", import.meta.url)), "utf8"),
);

// ---- generated-schema conformance pin (§A.2 / §A.1.1) --------------------

// The normative discriminant table, pinned independently on the consumer side.
// The generated tables (rendered from ssc-wire) must equal these exact pairs.
const PIN = [
  [0, "lex.excess-h-whitespace", "none", "target-only"],
  [1, "hyg.tab-in-body", "none", "target-only"],
  [2, "hyg.control-chars", "none", "target-only"],
  [3, "hyg.zero-width-misuse", "none", "target-only"],
  [4, "hyg.empty-verse", "none", "target-only"],
  [5, "hyg.invalid-codepoint", "none", "target-only"],
  [6, "hyg.replacement-run", "none", "target-only"],
  [7, "prop.length-ratio", "count-pair", "target-and-reference-silent-when-absent"],
  [8, "struct.source-marker-leftover", "none", "target-only"],
  [9, "struct.merge-conflict-marker", "none", "target-only"],
  [10, "punct.adjacency-anomaly", "count-pair", "target-only"],
  [11, "lex.duplicate-word", "none", "target-only"],
  [12, "lex.punct-only-token", "count-pair", "target-only"],
  [13, "uni.combining-mark-without-base", "none", "target-only"],
  [14, "uni.redundant-zero-width-space", "none", "target-only"],
  [15, "uni.mixed-script-in-token", "count-pair", "target-only"],
  [16, "lex.repeated-character-run", "u32", "target-only"],
  [17, "uni.mixed-numeral-systems", "none", "target-only"],
  [18, "punct.bracket-balance", "count-pair", "target-only"],
  [19, "punct.spacing-anomaly", "count-pair", "target-only"],
  [20, "case.sentence-initial-lowercase", "count-pair", "target-only"],
  [21, "case.inconsistent-word-casing", "count-pair", "target-only"],
  [22, "uni.rare-glyph", "u32", "target-only"],
  [23, "case.mixed-case-word", "count-pair", "target-only"],
  [24, "uni.mixed-normalization", "u32", "target-only"],
];

test("generated schema tables equal the pinned §A.2/§A.1.1 mapping", () => {
  assert.equal(Object.keys(CODE_TO_RULE).length, PIN.length, "one-to-one coverage");
  for (const [code, rule, digest, dep] of PIN) {
    assert.equal(CODE_TO_RULE[code], rule, `CODE_TO_RULE[${code}]`);
    assert.equal(RULE_TO_CODE[rule], code, `RULE_TO_CODE[${rule}]`);
    assert.equal(CODE_TO_DIGEST[code], digest, `CODE_TO_DIGEST[${code}]`);
    assert.equal(CODE_TO_INPUT_DEPENDENCY[code], dep, `CODE_TO_INPUT_DEPENDENCY[${code}]`);
  }
});

// ---- cross-language parity (Rust encoder -> JS decoder) -------------------

test("Rust vectors decode to the Rust decoder's values", () => {
  for (const v of VECTORS.valid) {
    const snap = decodeFindings(hexToBytes(v.hex), v.keys);
    assert.equal(snap.analysisId, BigInt(v.expected.analysisId), v.name);
    assert.equal(snap.targetContextId, BigInt(v.expected.targetContextId), v.name);
    assert.equal(snap.hasReference, v.expected.hasReference, v.name);
    assert.equal(snap.findings.length, v.expected.findings.length, v.name);
    snap.findings.forEach((f, i) => {
      const e = v.expected.findings[i];
      assert.equal(f.sid, e.sid, `${v.name}[${i}].sid`);
      assert.equal(f.code, e.code, `${v.name}[${i}].code`);
      assert.equal(f.severity, e.severity, `${v.name}[${i}].severity`);
      assert.equal(f.start, e.start, `${v.name}[${i}].start`);
      assert.equal(f.end, e.end, `${v.name}[${i}].end`);
      // score is quantized; compare within a quantum
      if (e.score === null) assert.equal(f.score, null, `${v.name}[${i}].score`);
      else assert.ok(Math.abs(f.score - e.score) <= 0.5 / 65535 + 1e-7, `${v.name}[${i}].score`);
      assert.equal(f.hasArgs, e.hasArgs, `${v.name}[${i}].hasArgs`);
      assert.deepEqual(f.digest, e.digest, `${v.name}[${i}].digest`);
      assert.equal(f.inputDependency, e.inputDependency, `${v.name}[${i}].dep`);
    });
  }
});

test("Rust malformed vectors are rejected by the JS decoder", () => {
  for (const v of VECTORS.malformed) {
    // keys are irrelevant: rejection happens during header/record validation.
    assert.throws(() => decodeFindings(hexToBytes(v.hex), []), v.name);
  }
});

// ---- decode validation (independent of the Rust vectors) ------------------

const T = 111n;
const A = 222n;

test("decode rejects a too-short buffer and non-Uint8Array", () => {
  assert.throws(() => decodeFindings(new Uint8Array(8), []));
  assert.throws(() => decodeFindings([], []));
});

test("decode rejects an out-of-range key index", () => {
  const bytes = encode({
    analysisId: A,
    targetContextId: T,
    hasReference: false,
    records: [{ rule: "lex.excess-h-whitespace", severity: "warning", keyIdx: 5, start: 0, end: 1 }],
  });
  assert.throws(() => decodeFindings(bytes, ["GEN 1:1"]));
});

test("empty buffer decodes to zero findings", () => {
  const snap = decodeFindings(
    encode({ analysisId: A, targetContextId: T, hasReference: false, records: [] }),
    [],
  );
  assert.equal(snap.findings.length, 0);
  assert.equal(snap.analysisId, A);
});

test("payload_saturated is exposed for a clamped count-pair", () => {
  const bytes = encode({
    analysisId: A,
    targetContextId: T,
    hasReference: false,
    records: [
      {
        rule: "punct.bracket-balance",
        severity: "error",
        keyIdx: 0,
        start: 0,
        end: 1,
        score: 0.99,
        hasArgs: true,
        digest: { shape: "count-pair", a: 65535, b: 5, saturated: true },
      },
    ],
  });
  const snap = decodeFindings(bytes, ["GEN 1:1"]);
  assert.deepEqual(snap.findings[0].digest, { shape: "count-pair", a: 65535, b: 5, saturated: true });
});

// ---- reconciliation (§A.5.5) ----------------------------------------------

const recSpec = (over = {}) => ({
  rule: "lex.excess-h-whitespace",
  severity: "warning",
  keyIdx: 0,
  start: 0,
  end: 1,
  ...over,
});

function snapOf(records, keys, { analysisId = A } = {}) {
  return decodeFindings(
    encode({ analysisId, targetContextId: T, hasReference: false, records }),
    keys,
  );
}

test("reconcile returns the exact prior array when nothing visible changed", () => {
  const keys = ["GEN 1:1", "GEN 1:2"];
  const recs = [recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1, rule: "hyg.tab-in-body" })];
  const prev = snapOf(recs, keys);
  const next = reconcileFindings(prev, encode({ analysisId: 999n, targetContextId: T, hasReference: false, records: recs }), keys);
  assert.equal(next.findings, prev.findings, "exact same array reference");
  assert.equal(next.analysisId, 999n, "new snapshot carries the new id");
});

test("reconcile reuses unchanged objects and replaces the changed one", () => {
  const keys = ["GEN 1:1", "GEN 1:2"];
  const prev = snapOf([recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1, rule: "hyg.tab-in-body" })], keys);
  // second record's severity changes (same identity: same sid/code/span)
  const next = reconcileFindings(
    prev,
    encode({
      analysisId: A,
      targetContextId: T,
      hasReference: false,
      records: [recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1, rule: "hyg.tab-in-body", severity: "error" })],
    }),
    keys,
  );
  assert.notEqual(next.findings, prev.findings);
  assert.equal(next.findings[0], prev.findings[0], "unchanged object reused");
  assert.notEqual(next.findings[1], prev.findings[1], "changed object replaced");
  assert.equal(next.findings[1].severity, "error");
});

test("rebased key_idx after an early insert does not churn identity", () => {
  // prev: GEN 1:1, GEN 1:2 at keyIdx 0,1
  const prev = snapOf(
    [recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1, rule: "hyg.tab-in-body" })],
    ["GEN 1:1", "GEN 1:2"],
  );
  // a verse inserted at the front shifts every later key_idx by 1, but the sids
  // and everything visible are unchanged -> identity holds, prior array reused.
  const next = reconcileFindings(
    prev,
    encode({
      analysisId: A,
      targetContextId: T,
      hasReference: false,
      records: [recSpec({ keyIdx: 1 }), recSpec({ keyIdx: 2, rule: "hyg.tab-in-body" })],
    }),
    ["GEN 1:0", "GEN 1:1", "GEN 1:2"],
  );
  assert.equal(next.findings, prev.findings, "reused exact array despite key_idx rebase");
});

test("duplicate-key occurrence ordinal keeps two identical findings distinct", () => {
  // GEN 1:1 appears twice; a finding on each physical verse, same code/span.
  const keys = ["GEN 1:1", "GEN 1:1"];
  const prev = snapOf([recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1 })], keys);
  // change only the SECOND occurrence's severity
  const next = reconcileFindings(
    prev,
    encode({
      analysisId: A,
      targetContextId: T,
      hasReference: false,
      records: [recSpec({ keyIdx: 0 }), recSpec({ keyIdx: 1, severity: "error" })],
    }),
    keys,
  );
  assert.equal(next.findings[0], prev.findings[0], "first occurrence reused");
  assert.notEqual(next.findings[1], prev.findings[1], "second occurrence replaced");
});

test("reconcile object reuse agrees with a slow multiset oracle (insert/delete/reorder)", () => {
  const keys = ["GEN 1:1", "GEN 1:2", "GEN 1:3"];
  const prevRecs = [
    recSpec({ keyIdx: 0, rule: "lex.excess-h-whitespace" }),
    recSpec({ keyIdx: 1, rule: "hyg.tab-in-body" }),
    recSpec({ keyIdx: 2, rule: "hyg.control-chars" }),
  ];
  // reorder + delete middle + insert a new one
  const nextRecs = [
    recSpec({ keyIdx: 2, rule: "hyg.control-chars" }),
    recSpec({ keyIdx: 0, rule: "lex.excess-h-whitespace" }),
    recSpec({ keyIdx: 1, rule: "hyg.empty-verse" }),
  ];
  const prev = snapOf(prevRecs, keys);
  const next = reconcileFindings(prev, encode({ analysisId: A, targetContextId: T, hasReference: false, records: nextRecs }), keys);

  // slow oracle: pair by (sid,code,start,end) multiset in record order
  const idOf = (f) => `${f.sid}|${f.code}|${f.start}|${f.end}`;
  const prevSet = new Set(prev.findings);
  const pool = new Map();
  prev.findings.forEach((f) => {
    const k = idOf(f);
    if (!pool.has(k)) pool.set(k, []);
    pool.get(k).push(f);
  });
  next.findings.forEach((f, i) => {
    const k = idOf(f);
    const q = pool.get(k);
    const expectReuse = q && q.length ? q.shift() : null;
    if (expectReuse) assert.equal(next.findings[i], expectReuse, `pos ${i} reused`);
    else assert.equal(prevSet.has(next.findings[i]), false, `pos ${i} is a fresh object`);
  });
});

// ---- persistence (§A.5.5 / §10.1) -----------------------------------------

test("decodePersistedFindings accepts an exact identity triple", () => {
  const keys = ["GEN 1:1"];
  const bytes = encode({ analysisId: A, targetContextId: T, hasReference: false, records: [recSpec()] });
  const snap = decodePersistedFindings(bytes, keys, { analysisId: A, targetContextId: T, hasReference: false });
  assert.equal(snap.provenance, "live");
  assert.equal(snap.findings.length, 1);
});

test("reference-removal salvage filters silent rows and equals a fresh no-ref decode", () => {
  const keys = ["GEN 1:1", "GEN 1:2"];
  // saved WITH reference: a target-only row + a reference-silent length-ratio row
  const savedBytes = encode({
    analysisId: 500n, // old reference-present id (irrelevant to salvage)
    targetContextId: T,
    hasReference: true,
    records: [
      recSpec({ keyIdx: 0, rule: "lex.excess-h-whitespace" }),
      recSpec({ keyIdx: 1, rule: "prop.length-ratio", score: 0.7, hasArgs: true, digest: { shape: "count-pair", a: 312, b: 0, saturated: false } }),
    ],
  });
  const currentNoRefId = 777n;
  const salvaged = decodePersistedFindings(savedBytes, keys, {
    analysisId: currentNoRefId,
    targetContextId: T,
    hasReference: false,
  });
  assert.equal(salvaged.provenance, "reference-removed");
  assert.equal(salvaged.analysisId, currentNoRefId, "adopts current no-ref id");
  assert.equal(salvaged.findings.length, 1, "the length-ratio row is dropped");
  assert.equal(salvaged.findings[0].code, "lex.excess-h-whitespace");

  // equals a fresh no-reference decode (only the target-only row remains)
  const fresh = decodeFindings(
    encode({ analysisId: currentNoRefId, targetContextId: T, hasReference: false, records: [recSpec({ keyIdx: 0, rule: "lex.excess-h-whitespace" })] }),
    keys,
  );
  assert.deepEqual(salvaged.findings, fresh.findings);
});

test("decodePersistedFindings rejects every non-salvageable mismatch", () => {
  const keys = ["GEN 1:1"];
  const noRef = encode({ analysisId: A, targetContextId: T, hasReference: false, records: [recSpec()] });
  const withRef = encode({ analysisId: A, targetContextId: T, hasReference: true, records: [recSpec()] });

  // changed analysis id (same tcid, both no-ref) -> reject (not the salvage case)
  assert.throws(() => decodePersistedFindings(noRef, keys, { analysisId: 999n, targetContextId: T, hasReference: false }));
  // changed target-context id -> reject
  assert.throws(() => decodePersistedFindings(noRef, keys, { analysisId: A, targetContextId: 999n, hasReference: false }));
  // absent -> present (saved no-ref, expected has-ref) -> reject
  assert.throws(() => decodePersistedFindings(noRef, keys, { analysisId: A, targetContextId: T, hasReference: true }));
  // changed reference (saved has-ref, expected has-ref, ids differ) -> reject
  assert.throws(() => decodePersistedFindings(withRef, keys, { analysisId: 999n, targetContextId: T, hasReference: true }));
  // malformed wire rejects before any identity check
  const bad = noRef.slice();
  bad[0] = 0;
  assert.throws(() => decodePersistedFindings(bad, keys, { analysisId: A, targetContextId: T, hasReference: false }));
});
