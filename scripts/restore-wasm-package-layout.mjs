import { copyFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// wasm-pack writes a `.gitignore` of `*` into each out-dir, which would
// exclude the generated artifacts we deliberately commit (the package is
// consumed as a GitHub dependency, not from npm). Restore a permissive
// ignore after each build. Mirrors usfm_onion's setup.
//
// It also copies the official packed-findings JS surface (the reviewed
// decoder/reconciler `findings.js` plus its generated schema/types) into each
// package dir, since wasm-pack only emits the bindgen glue. These are pure JS
// (no wasm), target-agnostic, and exported at `./findings`. Regenerate the
// generated files first with `cargo xtask wire-js`.

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const PACKAGE_DIRS = ["pkg-web", "pkg-bundler"];
const WIRE_JS_SRC = join(ROOT, "crates", "wasm", "js");
const WIRE_JS_FILES = [
  "findings.js",
  "findings.generated.js",
  "findings.generated.d.ts",
  "findings.d.ts",
];
const GITIGNORE_CONTENT = `# Intentionally checked in: downstream consumes these generated artifacts.
# wasm-pack rewrites this file during build, so restore it after each package build.

node_modules
.DS_Store

!.gitignore
!README.md
!package.json
!*.d.ts
!*.js
!*.wasm
`;

for (const dir of PACKAGE_DIRS) {
  writeFileSync(join(ROOT, dir, ".gitignore"), GITIGNORE_CONTENT);
  for (const file of WIRE_JS_FILES) {
    copyFileSync(join(WIRE_JS_SRC, file), join(ROOT, dir, file));
  }
}
