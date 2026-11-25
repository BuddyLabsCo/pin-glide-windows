import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { releaseIdentity, preparePublication, publishRelease, readCandidate, recoverCandidate, runPublication } from "./production-publication.mjs";

const commit = "a".repeat(40);
const newerCommit = "b".repeat(40);
const changelog = "# Changelog\n\n## 0.1.0\n\n- First release\n\n## 0.0.1\n\n- Older\n";
const packageInfo = {
  version: "0.1.0",
  filenames: {
    signed: "Pin-Glide-Setup-x64.exe",
    unsigned: "Pin-Glide-Setup-x64-unsigned.exe",
  },
};
const identity = releaseIdentity(commit, packageInfo, changelog);
const candidate = {
  commit, version: "0.1.0", signing_mode: "unsigned",
  filename: "Pin-Glide-Setup-x64-unsigned.exe",
  sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
};
const tag = { object: { type: "commit", sha: commit } };
const annotatedSha = "c".repeat(40);
const annotatedTag = { object: { type: "tag", sha: annotatedSha } };
const installerBytes = Buffer.from("abc");

function writeCandidate(directory, manifest = candidate, bytes = installerBytes) {
  writeFileSync(join(directory, "release-candidate.json"), JSON.stringify(manifest));
  writeFileSync(join(directory, manifest.filename), bytes);
}

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), "publication-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  writeCandidate(directory);
  return directory;
}

function repository() {
  const state = { tag: null, annotated: null, release: null, assets: [], artifacts: [], writes: [] };
  const api = async (method, path, body) => {
    if (method === "GET") {
      if (path === "/git/ref/tags/v0.1.0") return state.tag;
      if (path === `/git/tags/${annotatedSha}`) return state.annotated;
      if (path.startsWith("/releases?")) return state.release ? [state.release] : [];
      if (path.startsWith("/releases/1/assets?")) return state.assets;
      if (path === "/actions/runs/123/artifacts?per_page=100&page=1") return { artifacts: state.artifacts };
      throw new Error(`Unexpected read ${path}`);
    }
    state.writes.push([method, path]);
    if (method === "POST" && path === "/git/refs") {
      assert.deepEqual(body, { ref: "refs/tags/v0.1.0", sha: commit });
      state.tag = { object: { type: "commit", sha: body.sha } };
      return state.tag;
    }
    if (method === "POST" && path === "/releases") {
      assert.equal(body.tag_name, "v0.1.0");
      assert.equal(body.target_commitish, commit);
      assert.equal(body.draft, true);
      state.release = { id: 1, ...body };
      return state.release;
    }
    if (method === "UPLOAD") {
      assert.equal(path, "/releases/1/assets");
      assert.equal(state.release.draft, true);
      assert.equal(body.filename, candidate.filename);
      assert.deepEqual(body.bytes, installerBytes);
      const digest = createHash("sha256").update(body.bytes).digest("hex");
      state.assets.push({ id: 2, name: body.filename, state: "uploaded", size: body.bytes.length, digest: `sha256:${digest}` });
      return state.assets[0];
    }
    if (method === "PATCH" && path === "/releases/1") {
      Object.assign(state.release, body);
      return state.release;
    }
    if (method === "DELETE" && path === "/releases/assets/2") {
      state.assets = [];
      return null;
    }
    throw new Error(`Unexpected ${method} ${path}`);
  };
  return { state, api };
}

function unexpectedDownload() {
  throw new Error("Artifact download unavailable");
}

test("release identity uses only the matching Herdr changelog section", () => {
  assert.equal(identity.tag, "v0.1.0");
  assert.equal(identity.notes, "- First release");
  assert.throws(() => releaseIdentity(commit, packageInfo, "## 0.2.0\n- Other"), /changelog/i);
  assert.throws(() => releaseIdentity(commit, { ...packageInfo, version: "0.1.0-beta" }, "## 0.1.0-beta\n- Other"), /version/i);
});

test("publication package metadata comes from the Windows packaging helpers", () => {
  const output = execFileSync("powershell.exe", [
    "-NoProfile", "-NonInteractive", "-File", "packaging/windows/production-package-info.ps1",
  ], { encoding: "utf8" });
  const info = JSON.parse(output);
  assert.match(info.version, /^\d+\.\d+\.\d+$/);
  assert.deepEqual(info.filenames, packageInfo.filenames);
});

test("candidate identity and digest must agree with the authorized release", (t) => {
  const directory = fixture(t);
  assert.deepEqual(readCandidate(directory, identity), { manifest: candidate, bytes: installerBytes });
  writeCandidate(directory, candidate, "different");
  assert.throws(() => readCandidate(directory, identity), /digest/i);
  writeCandidate(directory, { ...candidate, commit: newerCommit });
  assert.throws(() => readCandidate(directory, identity), /identity/i);
});

test("recover only the original run's available artifact and verify its bytes", async (t) => {
  const directory = fixture(t);
  let downloads = 0;
  let expired = false;
  const api = async (method, path) => {
    assert.equal(method, "GET");
    assert.equal(path, "/actions/runs/123/artifacts?per_page=100&page=1");
    return { artifacts: [{ name: "production-release-candidate", expired }] };
  };
  const download = async (runId, name, destination) => {
    assert.equal(runId, "123");
    assert.equal(name, "production-release-candidate");
    assert.equal(destination, directory);
    downloads++;
    writeCandidate(directory, candidate, downloads === 1 ? "abc" : "corrupt");
  };
  assert.deepEqual(await recoverCandidate(api, directory, identity, "123", download), { manifest: candidate, bytes: installerBytes });
  await assert.rejects(recoverCandidate(api, directory, identity, "123", download), /digest/i);
  expired = true;
  assert.equal(await recoverCandidate(api, directory, identity, "123", download), null);
  assert.equal(downloads, 2);
});

test("publication creates a tag, stages verified bytes in a draft, and publishes once", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  assert.equal(await preparePublication(api, identity, directory, "123", unexpectedDownload), "build");
  await publishRelease(api, identity, directory);
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "UPLOAD", "PATCH"]);
  assert.equal(state.release.draft, false);
  assert.deepEqual(state.tag, tag);
  const withoutArtifacts = (method, path, body) => {
    if (path.startsWith("/actions/")) throw new Error("Artifact service unavailable");
    return api(method, path, body);
  };
  assert.equal(await preparePublication(withoutArtifacts, identity, directory, "123", unexpectedDownload), "complete");
  await publishRelease(api, identity, directory);
  assert.equal(state.writes.length, 4);
});

test("incorrect uploaded digest cannot publish the draft", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    const response = await api(...args);
    if (args[0] === "UPLOAD") response.digest = `sha256:${"b".repeat(64)}`;
    return response;
  }, identity, directory), /asset/i);
  assert.equal(state.release.draft, true);
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "UPLOAD"]);
});

test("a failed upload leaves a recoverable draft tied to the original candidate", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "UPLOAD") throw new Error("upload interrupted");
    return api(...args);
  }, identity, directory), /interrupted/);
  assert.equal(state.release.draft, true);
  await assert.rejects(preparePublication(api, identity, directory, "123", unexpectedDownload), /candidate/i);
  const different = Buffer.from("different");
  writeCandidate(directory, { ...candidate, sha256: createHash("sha256").update(different).digest("hex") }, different);
  await assert.rejects(publishRelease(api, identity, directory), /candidate/i);
  writeCandidate(directory);
  await publishRelease(api, identity, directory);
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "UPLOAD", "PATCH"]);
});

test("an uploaded draft can publish after the original artifact expires", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "PATCH") throw new Error("publication interrupted");
    return api(...args);
  }, identity, directory), /interrupted/);
  rmSync(join(directory, "release-candidate.json"));
  assert.equal(await preparePublication(api, identity, directory, "123", unexpectedDownload), "publish");
  await publishRelease(api, identity, directory);
  assert.equal(state.release.draft, false);
  assert.equal(state.writes.filter(([method]) => method === "UPLOAD").length, 1);
});

test("conflicting tags and existing asset digests never cause mutations", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  state.tag = { object: { type: "commit", sha: newerCommit } };
  await assert.rejects(publishRelease(api, identity, directory), /tag/i);
  assert.equal(state.writes.length, 0);
  state.tag = tag;
  await publishRelease(api, identity, directory);
  state.assets[0].digest = `sha256:${"b".repeat(64)}`;
  await assert.rejects(publishRelease(api, identity, directory), /asset/i);
  assert.equal(state.writes.length, 3);
});

test("resume matching lightweight and annotated tags without moving them", async (t) => {
  const directory = fixture(t);
  for (const existing of [tag, annotatedTag]) {
    const { api, state } = repository();
    state.tag = existing;
    state.annotated = tag;
    await publishRelease(api, identity, directory);
    assert.deepEqual(state.writes.map(([method]) => method), ["POST", "UPLOAD", "PATCH"]);
    assert.deepEqual(state.tag, existing);
  }
});

test("reject an annotated tag pointing at another commit", async (t) => {
  const { api, state } = repository();
  state.tag = annotatedTag;
  state.annotated = { object: { type: "commit", sha: newerCommit } };
  await assert.rejects(publishRelease(api, identity, fixture(t)), /tag/i);
  assert.equal(state.writes.length, 0);
});

test("retry an empty starter asset only while the release is a matching draft", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "UPLOAD") {
      state.assets = [{ id: 2, name: candidate.filename, state: "starter", size: 0 }];
      throw new Error("upload interrupted");
    }
    return api(...args);
  }, identity, directory), /interrupted/);
  await publishRelease(api, identity, directory);
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "DELETE", "UPLOAD", "PATCH"]);
  state.assets[0].state = "starter";
  state.assets[0].size = 0;
  await assert.rejects(publishRelease(api, identity, directory), /asset/i);
  assert.equal(state.writes.length, 5);
});

test("reject a release claiming the version under another tag before any mutation", async (t) => {
  const { api, state } = repository();
  state.release = { id: 1, name: "v0.1.0", tag_name: "v0.2.0", draft: false };
  await assert.rejects(publishRelease(api, identity, fixture(t)), /release/i);
  assert.equal(state.writes.length, 0);
});

test("reject a version's conflicting notes or extra assets without modifying its release", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  await publishRelease(api, identity, directory);
  const body = state.release.body;
  state.release.body = "Unrelated release notes";
  await assert.rejects(preparePublication(api, identity, directory, "123", unexpectedDownload), /notes/i);
  state.release.body = body;
  state.assets.push({ id: 3, name: "other.exe" });
  await assert.rejects(publishRelease(api, identity, directory), /asset/i);
  assert.equal(state.writes.length, 4);
});

test("workflow commands recover the original authorized checkout, not advanced main, and reject corrupt bytes", async (t) => {
  const directory = fixture(t);
  const { api, state } = repository();
  state.artifacts = [{ name: "production-release-candidate", expired: false }];
  const env = {
    GITHUB_EVENT_NAME: "pull_request", AUTHORIZED_COMMIT: commit, GITHUB_SHA: commit,
    GITHUB_RUN_ID: "123", GITHUB_OUTPUT: join(directory, "output"),
  };
  const context = {
    env, commit: newerCommit, packageInfo, changelog, api, directory,
    download: (runId, name, destination) => {
      assert.equal(runId, "123");
      assert.equal(name, "production-release-candidate");
      writeCandidate(destination);
    },
  };
  await assert.rejects(runPublication("prepare", context), /checkout/i);
  await assert.rejects(runPublication("publish", context), /checkout/i);
  assert.equal(state.writes.length, 0);
  context.commit = commit;
  await runPublication("prepare", context);
  assert.equal(readFileSync(env.GITHUB_OUTPUT, "utf8"), "plan=publish\n");
  writeCandidate(directory, candidate, "corrupt");
  await assert.rejects(runPublication("publish", context), /digest/i);
  assert.equal(state.writes.length, 0);
  await runPublication("prepare", context);
  await runPublication("publish", context);
  assert.deepEqual(state.tag, tag);
  assert.equal(state.release.draft, false);
});
