// Real-wasm Galley identity test (granularity-spine Appendix A §A.5.3/§10.1).
// This is the seam the P1 escaped through: the package smoke never
// instantiated the real wasm Galley, so the missing `expectedAnalysisId` /
// `expectedTargetContextId` / `hasReference` exports were invisible. This test
// loads the BUILT pkg-web wasm, constructs a real Galley, and exercises the
// persistence-load identity path end to end. Run after a build:
//   npm run build:wasm && node --test crates/wasm/js/galley.test.mjs
//
// (Uses the pkg-web `web` target initialized in Node with the wasm bytes, the
// same init bench-wasm.mjs uses — a committed, durable alternative to the
// throwaway pkg-node smoke.)

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const ROOT = new URL("../../../", import.meta.url);
const wasm = await import(new URL("pkg-web/sous_chef_web.js", ROOT));
await wasm.default({ module_or_path: readFileSync(new URL("pkg-web/sous_chef_web_bg.wasm", ROOT)) });
const { decodeFindings, decodePersistedFindings } = await import(new URL("pkg-web/findings.js", ROOT));

const target = { keys: ["GEN 1:1", "GEN 1:2"], texts: ["a  b work", "work here"] };

test("the three identity accessors are exposed on the wasm Galley (the P1)", () => {
  const g = new wasm.Galley({ target });
  assert.equal(typeof g.expectedAnalysisId, "function", "expectedAnalysisId exported");
  assert.equal(typeof g.expectedTargetContextId, "function", "expectedTargetContextId exported");
  assert.equal(typeof g.hasReference, "function", "hasReference exported");
  // camelCase mutation verbs are exported too (naming adjudication).
  for (const m of ["updateBook", "updateChapter", "removeBooks", "replaceCorpus", "replaceSource", "updateConfig", "findingArgs", "findingsArgs"]) {
    assert.equal(typeof g[m], "function", `${m} exported (camelCase)`);
  }
});

test("expected identity is readable before analyze, feeds decodePersistedFindings, and matches the header after analyze", () => {
  // (1) a first session persists a buffer.
  const galleyA = new wasm.Galley({ target });
  const savedBytes = galleyA.analyze();

  // (2) a fresh session reads all three expected values BEFORE any analyze.
  const galleyB = new wasm.Galley({ target });
  const expectedAnalysisId = galleyB.expectedAnalysisId();
  const expectedTargetContextId = galleyB.expectedTargetContextId();
  const expectedHasReference = galleyB.hasReference();
  assert.equal(typeof expectedAnalysisId, "bigint", "id marshals u64 -> bigint");
  assert.equal(expectedHasReference, false);

  // (3) the persisted buffer is accepted against the pre-analyze expected identity.
  const persisted = decodePersistedFindings(savedBytes, target.keys, {
    analysisId: expectedAnalysisId,
    targetContextId: expectedTargetContextId,
    hasReference: expectedHasReference,
  });
  assert.equal(persisted.provenance, "live", "exact-identity match accepted before analyze");
  assert.ok(persisted.findings.length > 0);

  // (4) analyze; the header's two ids + flag equal the pre-analyze expected values.
  const liveBytes = galleyB.analyze();
  const header = decodeFindings(liveBytes, target.keys);
  assert.equal(header.analysisId, expectedAnalysisId, "header analysis id == expected");
  assert.equal(header.targetContextId, expectedTargetContextId, "header target-context id == expected");
  assert.equal(header.hasReference, expectedHasReference, "header has_reference flag == expected");

  // (5) mutate an input: expectedAnalysisId() MOVES to track the new inputs,
  //     while the last publication's header id (a frozen value) does not — the
  //     divergence that motivates the "expected" name.
  const publishedId = header.analysisId;
  galleyB.updateBook({ slug: "GEN", keys: ["GEN 1:1", "GEN 1:2"], texts: ["a  b work extra", "work here"] });
  const afterEdit = galleyB.expectedAnalysisId();
  assert.notEqual(afterEdit, publishedId, "expected id moved off the last published id after an edit");
  assert.notEqual(afterEdit, expectedAnalysisId, "expected id moved off its pre-edit value");
  // the published buffer's header id is immutable — re-decoding it is unchanged.
  assert.equal(decodeFindings(liveBytes, target.keys).analysisId, publishedId, "the published header id did not change");
});
