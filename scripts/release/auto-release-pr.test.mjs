import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { updateProposal } from "./auto-release-pr.mjs";

const before = "1".repeat(40);
const head = "2".repeat(40);
const branch = "3".repeat(40);
const manifest = "crates/windows-app/Cargo.toml";
const lockfile = "Cargo.lock";
const changelog = "CHANGELOG.md";
const cargo = version => `[package]\nname = "windows-app"\nversion = "${version}"\n\n[dependencies]\nother = "1.0.0"\n`;
const lock = version => `[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "windows-app"\nversion = "${version}"\ndependencies = ["other"]\n`;

function fixture({
  paths = ["crates/windows-app/src/main.rs"], tags = [], version = "0.1.0",
  proposed, merged, subject = "Improve Receiver", existing = null, rejectedPush = false,
} = {}) {
  const files = new Map([[manifest, cargo(version)], [lockfile, lock(version)], [changelog, existing]]);
  const calls = [];
  const prs = [];
  let body;
  function git(...args) {
    calls.push(args);
    switch (args[0]) {
      case "branch": return "main";
      case "status": return "";
      case "diff": return args.includes("--cached") ? changelog : paths.join("\0");
      case "rev-parse": return head;
      case "log": return args.includes("--no-merges") ? "Improve Receiver\nEarlier change" : subject;
      case "tag": return tags.join("\n");
      case "for-each-ref": return proposed ? branch : "";
      case "show": return cargo(proposed);
      case "add":
      case "-c": return "";
      case "push":
        if (rejectedPush) throw new Error("Remote branch changed");
        return "";
      default: throw new Error(`Unexpected git call ${args.join(" ")}`);
    }
  }
  function gh(args, input) {
    if (args[1] === "list") {
      return JSON.stringify(args.includes("open")
        ? proposed ? [{ number: 45, isCrossRepository: false }] : []
        : merged ? [{ mergeCommit: { oid: merged }, isCrossRepository: false }] : []);
    }
    prs.push(args);
    body = input;
    return "";
  }
  return {
    files, calls, prs, get body() { return body; },
    run: () => updateProposal({
      before, git, gh,
      read: path => files.get(path) ?? null,
      write: (path, text) => files.set(path, text),
    }),
  };
}

test("first proposal keeps the seeded version and creates publication-ready notes", () => {
  const repo = fixture();
  repo.run();
  assert.equal(repo.files.get(manifest), cargo("0.1.0"));
  assert.equal(repo.files.get(lockfile), lock("0.1.0"));
  assert.equal(repo.files.get(changelog), "# Changelog\n\n## 0.1.0\n\n- Improve Receiver\n- Earlier change\n");
  assert.equal(repo.prs[0][1], "create");
  assert.match(repo.body, /Proposed release 0\.1\.0/);
  assert.ok(repo.calls.some(args => args[0] === "push" && args.includes("HEAD:refs/heads/auto-release")));
});

test("stable tags determine the next patch, without relabeling dependencies", () => {
  const repo = fixture({
    version: "0.9.12", tags: ["v0.9.12", "v0.9.2", "v99.0.0-beta", "v01.0.0"],
    existing: "# Changelog\n\n## 0.9.12\n\n- Previous release\n",
  });
  repo.run();
  assert.equal(repo.files.get(manifest), cargo("0.9.13"));
  assert.equal(repo.files.get(lockfile), lock("0.9.13"));
  assert.match(repo.files.get(changelog), /## 0\.9\.13\n\n- Improve Receiver[\s\S]*## 0\.9\.12\n\n- Previous release/);
  assert.ok(repo.calls.some(args => args.includes("v0.9.12..HEAD")));
});

test("refresh preserves a higher maintainer version and updates only the existing PR", () => {
  const repo = fixture({ tags: ["v0.1.0"], proposed: "2.0.0" });
  repo.run();
  assert.equal(repo.files.get(manifest), cargo("2.0.0"));
  assert.equal(repo.files.get(lockfile), lock("2.0.0"));
  assert.match(repo.files.get(changelog), /## 2\.0\.0/);
  assert.deepEqual(repo.prs[0].slice(0, 3), ["pr", "edit", "45"]);
  assert.equal(repo.prs.length, 1);
  assert.ok(repo.calls.some(args => args.includes(`--force-with-lease=refs/heads/auto-release:${branch}`)));
});

test("ignored workflow pushes cannot open a PR but refresh an open proposal's metadata", () => {
  const ignored = fixture({ paths: [".github/workflows/ci.yml"] });
  ignored.run();
  assert.equal(ignored.prs.length, 0);
  assert.ok(!ignored.calls.some(args => args[0] === "push"));
  const open = fixture({ paths: [".github/workflows/ci.yml"], proposed: "1.0.0" });
  open.run();
  assert.deepEqual(open.prs[0].slice(0, 3), ["pr", "edit", "45"]);
  assert.match(open.body, /Proposed release 1\.0\.0/);
  assert.ok(!open.calls.some(args => args[0] === "push"));
});

test("release merges do not loop, but subsequent ordinary dependency changes are eligible", () => {
  for (const scenario of [
    { paths: [changelog], subject: "chore: propose release v0.1.0 (#45)" },
    { paths: [manifest, lockfile, changelog], merged: head, subject: "Edited squash title" },
  ]) {
    const repo = fixture(scenario);
    repo.run();
    assert.equal(repo.prs.length, 0);
    assert.ok(!repo.calls.some(args => args[0] === "push"));
  }
  const dependency = fixture({ paths: [manifest, lockfile], merged: before, tags: ["v0.1.0"] });
  dependency.run();
  assert.equal(dependency.files.get(manifest), cargo("0.1.1"));
  assert.equal(dependency.prs[0][1], "create");
});

test("a new eligible push while publication is pending does not reuse the authorized version", () => {
  const repo = fixture({
    version: "0.1.0",
    existing: "# Changelog\n\n## 0.1.0\n\n- Authorized first release\n",
  });
  repo.run();
  assert.equal(repo.files.get(manifest), cargo("0.1.1"));
  assert.match(repo.files.get(changelog), /## 0\.1\.1[\s\S]*## 0\.1\.0\n\n- Authorized first release/);
});

test("version updates preserve real Cargo files except for the product version lines", () => {
  const manifestText = readFileSync(new URL(`../../${manifest}`, import.meta.url), "utf8");
  const lockText = readFileSync(new URL(`../../${lockfile}`, import.meta.url), "utf8");
  const product = /name = "windows-app"\r?\nversion = "(\d+\.\d+\.\d+)"/;
  const current = product.exec(manifestText)[1];
  const repo = fixture({ tags: [`v${current}`], proposed: "999.0.0" });
  repo.files.set(manifest, manifestText);
  repo.files.set(lockfile, lockText);
  repo.run();
  const expected = text => text.replace(product, match => match.replace(current, "999.0.0"));
  assert.equal(repo.files.get(manifest), expected(manifestText));
  assert.equal(repo.files.get(lockfile), expected(lockText));
});

test("a rejected branch update never creates or edits a Release PR", () => {
  const repo = fixture({ proposed: "1.0.0", rejectedPush: true });
  assert.throws(repo.run, /Remote branch changed/);
  assert.deepEqual(repo.prs, []);
});
