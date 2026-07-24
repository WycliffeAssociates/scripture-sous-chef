// Package-resolution smoke test (granularity-spine Appendix A §A.6 step 4 /
// §A.4.2): the official JS wire surface must be reachable from the BUILT
// package, not just from the source tree. This closes the WP3a dangling-export
// state — before `npm run build:wasm`, the committed `pkg-*` dirs did not
// contain `findings*.js`/`.d.ts` and the `./findings` export resolved to
// nothing. Run after a build:
//   npm run build:wasm && node --test crates/wasm/js/package.test.mjs
//
// (findings.test.mjs covers decode/reconcile/persistence behavior against the
// source module; this proves the package layout ships that module.)

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ROOT = new URL("../../../", import.meta.url);
const VECTORS = JSON.parse(
  readFileSync(fileURLToPath(new URL("./__vectors__.json", import.meta.url)), "utf8"),
);
const hexToBytes = (hex) => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
};
const emptyVector = VECTORS.valid.find((v) => v.name === "empty");

test("the `./findings` package export resolves by name from the built package", async () => {
  // Exercises the root package.json `exports["./findings"]` map (the bundler
  // package it points at). Requires `npm run build:wasm` to have populated it.
  const mod = await import("scripture-sous-chef-web/findings");
  assert.equal(typeof mod.decodeFindings, "function");
  assert.equal(typeof mod.decodePersistedFindings, "function");
  assert.equal(typeof mod.reconcileFindings, "function");
  // The built copy actually runs: decode a committed Rust-encoder vector.
  const snap = mod.decodeFindings(hexToBytes(emptyVector.hex), emptyVector.keys);
  assert.equal(snap.findings.length, 0);
  assert.equal(snap.analysisId, BigInt(emptyVector.expected.analysisId));
});

test("both built package dirs ship the findings JS surface + declarations", async () => {
  for (const dir of ["pkg-bundler", "pkg-web"]) {
    for (const f of ["findings.js", "findings.generated.js", "findings.generated.d.ts", "findings.d.ts"]) {
      assert.ok(existsSync(fileURLToPath(new URL(`${dir}/${f}`, ROOT))), `${dir}/${f} is present`);
    }
    // The built copy loads and decodes from its own dir (web variant included).
    const mod = await import(new URL(`${dir}/findings.js`, ROOT));
    const snap = mod.decodeFindings(hexToBytes(emptyVector.hex), emptyVector.keys);
    assert.equal(snap.findings.length, 0);
  }
});
