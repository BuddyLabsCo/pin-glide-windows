import assert from "node:assert/strict";
import { resolve } from "node:path";
import { test } from "node:test";
import { dependencyIssues, handwrittenSources, sourceIssues } from "./check.mjs";

const rustFile = "crates/receiver-core/src/lib.rs";
const slintFile = "crates/windows-app/ui/app.slint";

test("handwritten discovery includes new tests and skips generated and archived sources", () => {
  assert.deepEqual(
    handwrittenSources([
      rustFile,
      rustFile,
      "crates/windows-app/tests/space name.rs",
      slintFile,
      "crates/windows-app/ui/vendor/widget.slint",
      "docs/prototypes/ui-tool/ui.slint",
      "generated/http-client/src/lib.rs",
      "",
    ].join("\0")),
    [rustFile, "crates/windows-app/tests/space name.rs", slintFile],
  );
});

test("500 physical lines pass and a 501st unterminated line fails", () => {
  const formatted = "line\r\n".repeat(500);
  assert.deepEqual(sourceIssues(rustFile, formatted), []);
  assert.deepEqual(sourceIssues(rustFile, ""), []);
  assert.match(sourceIssues(rustFile, `${formatted}last`)[0], /501 physical lines/);
});

test("Slint accepts CRLF and rejects differing format or a missing final newline", () => {
  const format = () => "component Demo {}\n";
  assert.deepEqual(sourceIssues(slintFile, "component Demo {}\r\n", format), []);
  assert.match(sourceIssues(slintFile, "component Demo{}", format)[0], /Slint format differs/);
  assert.match(sourceIssues(slintFile, "component Demo {}", format)[0], /Slint format differs/);
  assert.throws(
    () => sourceIssues(slintFile, "invalid", () => { throw new Error("invalid syntax"); }),
    /invalid syntax/,
  );
});

test("dependency policy rejects forbidden metadata even when renamed, optional or target-specific", () => {
  const workspace = resolve("fixture-workspace");
  const makePackage = (name, directory, dependencies) => ({
    id: name,
    name,
    manifest_path: resolve(workspace, directory, "Cargo.toml"),
    dependencies,
  });
  const coreEdge = { name: "receiver-core", path: resolve(workspace, "crates/receiver-core") };
  const slintEdge = {
    name: "slint",
    source: "registry+fixture",
    kind: "build",
    target: "cfg(windows)",
    optional: true,
    rename: "ui",
  };
  const generatedEdge = { name: "pin-glide-http", path: resolve(workspace, "generated/http-client") };
  const core = makePackage("receiver-core", "crates/receiver-core", []);
  const app = makePackage("windows-app", "crates/windows-app", [coreEdge, slintEdge, generatedEdge]);
  const http = makePackage("pin-glide-http", "generated/http-client", []);
  const metadata = {
    workspace_members: [core.id, app.id, http.id],
    workspace_root: workspace,
    packages: [core, app, http],
  };
  assert.deepEqual(dependencyIssues(metadata), []);
  core.dependencies.push(slintEdge);
  http.dependencies.push(coreEdge);
  generatedEdge.path = resolve(workspace, "elsewhere");
  assert.deepEqual(dependencyIssues(metadata), [
    "receiver-core: forbidden dependency slint",
    "windows-app: forbidden dependency pin-glide-http",
    "pin-glide-http: forbidden dependency receiver-core",
  ]);
});
