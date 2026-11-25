import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { releaseIdentity, publicationPlan, publishRelease, readCandidate, recoverCandidate } from "./production-publication.mjs";

const commit = "a".repeat(40);
const identity = releaseIdentity(commit, "0.1.0", "# Changelog\n\n## 0.1.0\n\n- First release\n\n## 0.0.1\n\n- Older\n");
const candidate = {
  commit, version: "0.1.0", signing_mode: "unsigned",
  filename: "Pin-Glide-Setup-x64-unsigned.exe",
  sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
};
const tag = { object: { type: "commit", sha: commit } };

function repository() {
  const state = { tag: null, release: null, assets: [], writes: [] };
  const api = async (method, path, body) => {
    if (method === "GET") {
      if (path === "/git/ref/tags/v0.1.0") return state.tag;
      if (path.startsWith("/releases?")) return state.release ? [state.release] : [];
      if (path.startsWith("/releases/1/assets?")) return state.assets;
    }
    state.writes.push([method, path]);
    if (method === "POST" && path === "/git/refs") {
      state.tag = { object: { type: "commit", sha: body.sha } };
      return state.tag;
    }
    if (method === "POST" && path === "/releases") {
      state.release = { id: 1, ...body };
      return state.release;
    }
    if (method === "UPLOAD") {
      state.assets.push({ id: 2, name: candidate.filename, state: "uploaded", size: 3, digest: `sha256:${candidate.sha256}` });
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

test("release identity uses only the matching Herdr changelog section", () => {
  assert.equal(identity.tag, "v0.1.0");
  assert.equal(identity.notes, "- First release");
  assert.throws(() => releaseIdentity(commit, "0.1.0", "## 0.2.0\n- Other"), /changelog/i);
  assert.throws(() => releaseIdentity(commit, "0.1.0-beta", "## 0.1.0-beta\n- Other"), /version/i);
});

test("candidate identity and digest must agree with the authorized release", () => {
  const directory = mkdtempSync(join(tmpdir(), "publication-"));
  try {
    writeFileSync(join(directory, "release-candidate.json"), JSON.stringify(candidate));
    writeFileSync(join(directory, candidate.filename), "abc");
    assert.deepEqual(readCandidate(directory, identity), candidate);
    writeFileSync(join(directory, candidate.filename), "different");
    assert.throws(() => readCandidate(directory, identity), /digest/i);
    writeFileSync(join(directory, "release-candidate.json"), JSON.stringify({ ...candidate, commit: "b".repeat(40) }));
    assert.throws(() => readCandidate(directory, identity), /identity/i);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("recover only the original run's available artifact and verify its bytes", async () => {
  const directory = mkdtempSync(join(tmpdir(), "publication-"));
  try {
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
      writeFileSync(join(directory, "release-candidate.json"), JSON.stringify(candidate));
      writeFileSync(join(directory, candidate.filename), downloads === 1 ? "abc" : "corrupt");
    };
    assert.deepEqual(await recoverCandidate(api, directory, identity, "123", download), candidate);
    await assert.rejects(recoverCandidate(api, directory, identity, "123", download), /digest/i);
    expired = true;
    assert.equal(await recoverCandidate(api, directory, identity, "123", download), null);
    assert.equal(downloads, 2);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("publication creates a tag, stages the installer in a draft, and publishes once", async () => {
  const { api, state } = repository();
  assert.equal(await publicationPlan(api, identity, null), "build");
  assert.equal(await publicationPlan(api, identity, candidate), "publish");
  await publishRelease(api, identity, candidate, "installer");
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "UPLOAD", "PATCH"]);
  assert.equal(state.release.draft, false);
  assert.deepEqual(state.tag, tag);
  assert.equal(await publicationPlan(api, identity, null), "complete");
  await publishRelease(api, identity, null);
  assert.equal(state.writes.length, 4);
});

test("a failed upload leaves a recoverable draft tied to the original candidate", async () => {
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "UPLOAD") throw new Error("upload interrupted");
    return api(...args);
  }, identity, candidate, "installer"), /interrupted/);
  assert.equal(state.release.draft, true);
  await assert.rejects(publicationPlan(api, identity, null), /candidate/i);
  await assert.rejects(publicationPlan(api, identity, { ...candidate, sha256: "b".repeat(64) }), /candidate/i);
  await publishRelease(api, identity, candidate, "installer");
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "UPLOAD", "PATCH"]);
});

test("an uploaded draft can publish after the original artifact expires", async () => {
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "PATCH") throw new Error("publication interrupted");
    return api(...args);
  }, identity, candidate, "installer"), /interrupted/);
  assert.equal(await publicationPlan(api, identity, null), "publish");
  await publishRelease(api, identity, null);
  assert.equal(state.release.draft, false);
  assert.equal(state.writes.filter(([method]) => method === "UPLOAD").length, 1);
});

test("conflicting tags and existing asset digests never cause mutations", async () => {
  const { api, state } = repository();
  state.tag = { object: { type: "commit", sha: "b".repeat(40) } };
  await assert.rejects(publishRelease(api, identity, candidate, "installer"), /tag/i);
  assert.equal(state.writes.length, 0);
  state.tag = tag;
  await publishRelease(api, identity, candidate, "installer");
  state.assets[0].digest = `sha256:${"b".repeat(64)}`;
  await assert.rejects(publishRelease(api, identity, candidate, "installer"), /asset/i);
  assert.equal(state.writes.length, 3);
});

test("resume a tag-only publication without moving the tag", async () => {
  const { api, state } = repository();
  state.tag = tag;
  await publishRelease(api, identity, candidate, "installer");
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "UPLOAD", "PATCH"]);
});

test("retry an empty starter asset only while the release is a matching draft", async () => {
  const { api, state } = repository();
  await assert.rejects(publishRelease(async (...args) => {
    if (args[0] === "UPLOAD") {
      state.assets = [{ id: 2, name: candidate.filename, state: "starter", size: 0 }];
      throw new Error("upload interrupted");
    }
    return api(...args);
  }, identity, candidate, "installer"), /interrupted/);
  await publishRelease(api, identity, candidate, "installer");
  assert.deepEqual(state.writes.map(([method]) => method), ["POST", "POST", "DELETE", "UPLOAD", "PATCH"]);
  state.assets[0].state = "starter";
  state.assets[0].size = 0;
  await assert.rejects(publishRelease(api, identity, candidate, "installer"), /asset/i);
  assert.equal(state.writes.length, 5);
});

test("reject a version's conflicting notes or extra assets without modifying its release", async () => {
  const { api, state } = repository();
  await publishRelease(api, identity, candidate, "installer");
  const body = state.release.body;
  state.release.body = "Unrelated release notes";
  await assert.rejects(publicationPlan(api, identity, candidate), /notes/i);
  state.release.body = body;
  state.assets.push({ id: 3, name: "other.exe" });
  await assert.rejects(publishRelease(api, identity, candidate, "installer"), /asset/i);
  assert.equal(state.writes.length, 4);
});
