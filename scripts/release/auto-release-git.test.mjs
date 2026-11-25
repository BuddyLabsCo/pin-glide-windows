import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import { releaseIdentity } from "../../packaging/windows/production-publication.mjs";
import { updateProposal } from "./auto-release-pr.mjs";

const manifest = "crates/windows-app/Cargo.toml";
const lockfile = "Cargo.lock";
const changelog = "CHANGELOG.md";
const releaseFiles = [manifest, lockfile, changelog];
const releaseRef = "refs/heads/auto-release";

function repository(t) {
  const directory = mkdtempSync(join(tmpdir(), "pin-glide-release-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const remote = join(directory, "remote.git");
  const main = join(directory, "source");
  const env = {
    ...process.env,
    GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_AUTHOR_NAME: "Fixture", GIT_AUTHOR_EMAIL: "fixture@example.invalid",
    GIT_COMMITTER_NAME: "Fixture", GIT_COMMITTER_EMAIL: "fixture@example.invalid",
  };
  const git = (cwd, ...args) => execFileSync("git", args, {
    cwd, env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  git(directory, "init", "--bare", "--initial-branch=main", remote);
  git(directory, "init", "--initial-branch=main", main);
  git(main, "remote", "add", "origin", remote);

  const write = (cwd, file, text) => {
    const path = join(cwd, file);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
  };
  function commit(files, subject) {
    for (const [file, text] of Object.entries(files)) write(main, file, text);
    git(main, "add", ".");
    git(main, "commit", "-m", subject);
    git(main, "push", "origin", "main");
    return git(main, "rev-parse", "HEAD");
  }
  const seed = commit({
    [manifest]: '[package]\nname = "windows-app"\nversion = "0.1.0"\n\n[dependencies]\nother = "1"\n',
    [lockfile]: '[[package]]\nname = "other"\nversion = "1.0.0"\n\n[[package]]\nname = "windows-app"\nversion = "0.1.0"\n',
    "README.md": "Initial Receiver\n",
  }, "Initial Receiver");

  let open = false;
  let merged = [];
  let body = "";
  let runNumber = 0;
  const changes = [];
  function gh(args, input) {
    if (args[1] === "list") {
      return JSON.stringify(args.includes("open")
        ? open ? [{ number: 45, isCrossRepository: false }] : []
        : merged.map(oid => ({ mergeCommit: { oid }, isCrossRepository: false })));
    }
    if (args[1] === "create") {
      assert.equal(open, false);
      open = true;
    } else {
      assert.equal(args[1], "edit");
      assert.equal(args[2], "45");
      assert.equal(open, true);
    }
    changes.push(args[1]);
    body = input;
    return "";
  }

  function run(before, after) {
    const checkout = join(directory, `run-${++runNumber}`);
    git(directory, "clone", "--branch", "main", remote, checkout);
    updateProposal({
      before, after, git: (...args) => git(checkout, ...args), gh,
      read: file => {
        try {
          return readFileSync(join(checkout, file), "utf8");
        } catch (error) {
          if (error.code === "ENOENT") return null;
          throw error;
        }
      },
      write: (file, text) => write(checkout, file, text),
    });
  }
  const show = file => git(remote, "show", `${releaseRef}:${file}`);
  const ref = () => git(remote, "rev-parse", releaseRef);
  return {
    seed, commit, run, changes, ref, show,
    get body() { return body; },
    parent: () => git(remote, "rev-parse", `${releaseRef}^`),
    notes: version => releaseIdentity(ref(), { version, filenames: {} }, show(changelog)).notes,
    authorize() {
      const sha = commit(Object.fromEntries(releaseFiles.map(file => [file, `${show(file)}\n`])),
        "Maintainer squash title");
      merged = [sha, ...merged];
      open = false;
      return sha;
    },
    publish(version, sha) {
      git(main, "tag", `v${version}`, sha);
      git(main, "push", "origin", `refs/tags/v${version}`);
    },
  };
}

test("delayed eligible pushes catch up to main and old reruns cannot regress the proposal", t => {
  const repo = repository(t);
  const first = repo.commit({ "README.md": "First feature\n" }, "First feature");
  const ignored = repo.commit({ ".github/workflows/ci.yml": "name: CI\n" }, "Tune CI");
  repo.run(first, ignored);
  assert.deepEqual(repo.changes, []);
  repo.run(repo.seed, first);
  assert.equal(repo.parent(), ignored);
  assert.equal(repo.notes("0.1.0"), "- Tune CI\n- First feature\n- Initial Receiver");
  assert.equal(repo.show(manifest).includes('version = "0.1.0"'), true);

  const second = repo.commit({ "README.md": "Second feature\n" }, "Second feature");
  repo.run(ignored, second);
  const newestNotes = repo.notes("0.1.0");
  assert.match(newestNotes, /Second feature/);
  repo.run(repo.seed, first);
  assert.equal(repo.parent(), second);
  assert.equal(repo.notes("0.1.0"), newestNotes);
  assert.match(repo.body, /Second feature/);
  assert.deepEqual(repo.changes, ["create", "edit", "edit"]);

  const branch = repo.ref();
  const anotherIgnored = repo.commit({ ".github/workflows/ci.yml": "name: Checks\n" }, "Rename CI");
  repo.run(second, anotherIgnored);
  assert.equal(repo.ref(), branch);
  assert.match(repo.body, /Rename CI/);
});

test("pending and published releases use the same authorized baseline and do not loop", t => {
  const repo = repository(t);
  const first = repo.commit({ "README.md": "Shipped feature\n" }, "Shipped feature");
  repo.run(repo.seed, first);
  const previousNotes = repo.notes("0.1.0");
  const authorized = repo.authorize();
  repo.run(first, authorized);
  repo.run(repo.seed, first);
  assert.deepEqual(repo.changes, ["create"]);

  const next = repo.commit({
    [manifest]: repo.show(manifest).replace('other = "1"', 'other = "2"') + "\n",
    [lockfile]: repo.show(lockfile).replace('version = "1.0.0"', 'version = "2.0.0"') + "\n",
  }, "chore: propose release v0.1.0 (#45)");
  repo.run(authorized, next);
  assert.equal(repo.parent(), next);
  assert.match(repo.show(manifest), /version = "0\.1\.1"/);
  assert.match(repo.show(lockfile), /name = "other"\nversion = "2\.0\.0"/);
  const notes = "- chore: propose release v0.1.0 (#45)";
  assert.equal(repo.notes("0.1.1"), notes);
  assert.equal(repo.notes("0.1.0"), previousNotes);
  assert.match(repo.body, new RegExp(`Changelog base: ${authorized}`));

  repo.publish("0.1.0", authorized);
  repo.run(authorized, next);
  assert.equal(repo.notes("0.1.1"), notes);
  assert.equal(repo.notes("0.1.0"), previousNotes);
  assert.match(repo.body, /Changelog base: v0\.1\.0/);
  assert.deepEqual(repo.changes, ["create", "create", "edit"]);
});

test("workflow checks out current main but supplies both original event SHAs", () => {
  const workflow = readFileSync(new URL("../../.github/workflows/auto-release-pr.yml", import.meta.url), "utf8");
  assert.match(workflow, /uses: actions\/checkout@v5\s+with:\s+ref: main/);
  assert.match(workflow, /PUSH_BEFORE: \$\{\{ github\.event\.before \}\}/);
  assert.match(workflow, /PUSH_AFTER: \$\{\{ github\.event\.after \}\}/);
});
