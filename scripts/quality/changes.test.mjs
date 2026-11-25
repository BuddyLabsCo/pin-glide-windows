import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { classifyChanges, classifyRange } from "./changes.mjs";

test("documentation and unrelated release automation do not compile Windows", () => {
  assert.deepEqual(classifyChanges(["docs/guide.md", "docs/examples/Cargo.toml", "code-architecture.md"]), {
    windows: false, packaging: false, release: false, advisories: false,
  });
  for (const path of ["scripts/release/auto-release-pr.mjs", ".github/workflows/auto-release-pr.yml"]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: false, packaging: false, release: true, advisories: false,
    }, path);
  }
});

test("application and generated sources require Windows without packaging", () => {
  for (const path of [
    "crates/receiver-core/src/lib.rs",
    "crates/windows-app/src/service/mod.rs",
    "crates/windows-app/ui/app.slint",
    "generated/wss-models/src/lib.rs",
    "generated/http-client/src/lib.rs",
    "clippy.toml",
    "rustfmt.toml",
  ]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: true, packaging: false, release: false, advisories: false,
    }, path);
  }
});

test("packaging inputs and shared build configuration require the real installer", () => {
  for (const path of [
    "packaging/windows/installer.nsi",
    "packaging/windows/production-policy.ps1",
    "crates/windows-app/build.rs",
    "crates/windows-app/ui/assets/app-icon.ico",
    "crates/windows-app/ui/assets/app-icon.png",
    ".github/workflows/development-packaging.yml",
    ".github/workflows/production-release.yml",
  ]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: true, packaging: true, release: false, advisories: false,
    }, path);
  }
  for (const path of [
    "Cargo.toml", "Cargo.lock", "crates/receiver-core/Cargo.toml",
    "crates/windows-app/Cargo.toml", "generated/http-client/Cargo.toml",
    "rust-toolchain.toml", "rust-toolchain", ".cargo/config.toml",
  ]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: true, packaging: true, release: false, advisories: true,
    }, path);
  }
  for (const path of ["scripts/quality/check.mjs", ".github/workflows/ci.yml", ".github/actions/quality/action.yml"]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: true, packaging: true, release: true, advisories: true,
    }, path);
  }
});

test("advisory policy and contract tooling need no Windows compilation", () => {
  assert.deepEqual(classifyChanges(["deny.toml"]), {
    windows: false, packaging: false, release: false, advisories: true,
  });
  for (const path of ["scripts/contracts/service.lock.json", ".github/workflows/contracts.yml"]) {
    assert.deepEqual(classifyChanges([path]), {
      windows: false, packaging: false, release: false, advisories: false,
    }, path);
  }
});

test("range classification includes removed files and both ends of renames", () => {
  const directory = mkdtempSync(join(tmpdir(), "pin-glide-changes-"));
  const git = (...args) => execFileSync("git", args, { cwd: directory, encoding: "utf8" }).trim();
  const add = (path) => {
    mkdirSync(join(directory, path, ".."), { recursive: true });
    writeFileSync(join(directory, path), path);
  };
  const commit = (message) => {
    git("add", "-A");
    git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "-c", "commit.gpgsign=false", "commit", "-qm", message);
    return git("rev-parse", "HEAD");
  };
  try {
    git("init", "-q");
    add("docs/guide.md");
    add("packaging/windows/old installer.nsi");
    add("crates/receiver-core/src/old.rs");
    const base = commit("base");
    renameSync(join(directory, "packaging/windows/old installer.nsi"), join(directory, "docs/renamed.nsi"));
    const renamed = commit("rename");
    assert.deepEqual(classifyRange(base, renamed, directory), {
      windows: true, packaging: true, release: false, advisories: false,
    });
    rmSync(join(directory, "crates/receiver-core/src/old.rs"));
    const deleted = commit("remove core");
    assert.deepEqual(classifyRange(renamed, deleted, directory), {
      windows: true, packaging: false, release: false, advisories: false,
    });
    mkdirSync(join(directory, "crates/windows-app/ui"), { recursive: true });
    renameSync(join(directory, "docs/guide.md"), join(directory, "crates/windows-app/ui/new.slint"));
    const moved = commit("move into app");
    assert.equal(classifyRange(deleted, moved, directory).windows, true);
    assert.deepEqual(classifyRange("missing-base", moved, directory), {
      windows: true, packaging: true, release: true, advisories: true,
    });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
