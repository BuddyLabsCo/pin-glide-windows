import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const artifactName = "production-release-candidate";
const manifestName = "release-candidate.json";
const marker = "\n\n<!-- verified-candidate: ";
const root = fileURLToPath(new URL("../..", import.meta.url));

export function releaseIdentity(commit, version, changelog) {
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error("Invalid authorized commit");
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error("Expected a stable Cargo product version");
  }
  const sections = changelog.replace(/\r\n/g, "\n").split(/^## /m).slice(1);
  const matching = sections.filter(section => section.split("\n")[0] === version);
  const notes = matching[0]?.slice(version.length).trim();
  if (matching.length !== 1 || !notes || notes.includes(marker)) {
    throw new Error(`Expected one nonempty changelog section for ${version}`);
  }
  return { commit, version, tag: `v${version}`, notes };
}

function validateManifest(manifest, identity) {
  const { commit, version, signing_mode, filename, sha256 } = manifest;
  const expectedFilename = signing_mode === "unsigned"
    ? "Pin-Glide-Setup-x64-unsigned.exe" : "Pin-Glide-Setup-x64.exe";
  if (commit !== identity.commit || version !== identity.version ||
      !["signed", "unsigned"].includes(signing_mode) || filename !== expectedFilename ||
      typeof sha256 !== "string" || !/^[a-f0-9]{64}$/.test(sha256)) {
    throw new Error("Verified candidate identity conflicts with authorized release");
  }
  return { commit, version, signing_mode, filename, sha256 };
}

export function readCandidate(directory, identity) {
  const manifest = validateManifest(JSON.parse(readFileSync(join(directory, manifestName), "utf8")), identity);
  const bytes = readFileSync(join(directory, manifest.filename));
  if (createHash("sha256").update(bytes).digest("hex") !== manifest.sha256) {
    throw new Error("Verified candidate digest mismatch");
  }
  return manifest;
}

function releaseBody(identity, candidate) {
  return `${identity.notes}${marker}${JSON.stringify(candidate)} -->`;
}

async function list(api, path, collection) {
  const results = [];
  for (let page = 1; ; page++) {
    const response = await api("GET", `${path}?per_page=100&page=${page}`);
    const items = collection ? response[collection] : response;
    results.push(...items);
    if (items.length < 100) return results;
  }
}

async function inspect(api, identity, candidate) {
  const tag = await api("GET", `/git/ref/tags/${identity.tag}`);
  if (tag && (tag.object.type !== "commit" || tag.object.sha !== identity.commit)) {
    throw new Error(`Conflicting tag ${identity.tag}`);
  }
  const releases = (await list(api, "/releases")).filter(release => release.tag_name === identity.tag);
  if (releases.length > 1) throw new Error("Conflicting releases for version");
  const release = releases[0];
  if (!release) return { tag, candidate };
  if (!tag || release.name !== identity.tag ||
      release.prerelease !== false || typeof release.draft !== "boolean") {
    throw new Error("Conflicting Release identity");
  }
  const prefix = `${identity.notes}${marker}`;
  if (!release.body?.startsWith(prefix) || !release.body.endsWith(" -->")) {
    throw new Error("Conflicting Release notes or missing verified candidate identity");
  }
  const recorded = validateManifest(JSON.parse(release.body.slice(prefix.length, -4)), identity);
  if (candidate && JSON.stringify(validateManifest(candidate, identity)) !== JSON.stringify(recorded)) {
    throw new Error("Existing Release conflicts with verified candidate");
  }
  const assets = await list(api, `/releases/${release.id}/assets`);
  if (assets.length > 1 || (assets.length === 1 && assets[0].name !== recorded.filename)) {
    throw new Error("Conflicting Release asset identity");
  }
  const asset = assets[0];
  const uploaded = asset?.state === "uploaded" && asset.size > 0 && asset.digest === `sha256:${recorded.sha256}`;
  const starter = release.draft && asset?.state === "starter" && asset.size === 0;
  if ((asset && !uploaded && !starter) || (!release.draft && !uploaded)) {
    throw new Error("Conflicting Release asset digest or state");
  }
  if (!uploaded && !candidate) throw new Error("Original verified candidate is required to recover this draft");
  return { tag, release, candidate: recorded, uploaded, starter: starter ? asset : null };
}

export async function publicationPlan(api, identity, candidate) {
  const state = await inspect(api, identity, candidate);
  if (state.release && !state.release.draft) return "complete";
  return state.uploaded || candidate ? "publish" : "build";
}

export async function publishRelease(api, identity, candidate, installer) {
  let state = await inspect(api, identity, candidate);
  if (state.release && !state.release.draft) return;
  if (!state.uploaded && (!candidate || !installer)) throw new Error("Verified candidate required for publication");
  if (!state.tag) {
    await api("POST", "/git/refs", { ref: `refs/tags/${identity.tag}`, sha: identity.commit });
  }
  if (!state.release) {
    await api("POST", "/releases", {
      tag_name: identity.tag, target_commitish: identity.commit, name: identity.tag,
      body: releaseBody(identity, validateManifest(candidate, identity)), draft: true, prerelease: false,
    });
  }
  state = await inspect(api, identity, candidate);
  if (state.release && !state.release.draft) return;
  if (!state.uploaded) {
    if (state.starter) await api("DELETE", `/releases/assets/${state.starter.id}`);
    await api("UPLOAD", `/releases/${state.release.id}/assets`, { filename: candidate.filename, installer });
  }
  state = await inspect(api, identity, candidate);
  if (!state.uploaded) throw new Error("Installer upload was not verified");
  if (state.release.draft) {
    await api("PATCH", `/releases/${state.release.id}`, { draft: false });
  }
  if ((await inspect(api, identity, candidate)).release.draft) {
    throw new Error("Release remains a draft");
  }
}

function githubApi(repository, token) {
  if (!token || !/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error("Missing GitHub publication configuration");
  return async (method, path, body) => {
    const upload = method === "UPLOAD";
    const url = upload
      ? `https://uploads.github.com/repos/${repository}${path}?name=${encodeURIComponent(body.filename)}`
      : `https://api.github.com/repos/${repository}${path}`;
    const response = await fetch(url, {
      method: upload ? "POST" : method,
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        ...(body ? { "Content-Type": upload ? "application/octet-stream" : "application/json" } : {}),
      },
      ...(body ? { body: upload ? readFileSync(body.installer) : JSON.stringify(body) } : {}),
      redirect: "error",
    });
    if (method === "GET" && path.startsWith("/git/ref/tags/") && response.status === 404) return null;
    if (!response.ok) throw new Error(`GitHub ${method} ${path} failed (${response.status})`);
    return response.status === 204 ? null : response.json();
  };
}

export async function recoverCandidate(api, directory, identity, runId, download) {
  if (!/^\d+$/.test(runId)) throw new Error("Invalid original workflow run ID");
  const artifacts = (await list(api, `/actions/runs/${runId}/artifacts`, "artifacts")).filter(artifact =>
    artifact.name === artifactName && !artifact.expired);
  if (!artifacts.length) return null;
  if (artifacts.length !== 1) throw new Error("Multiple verified candidate artifacts in original run");
  await download(runId, artifactName, directory);
  return readCandidate(directory, identity);
}

async function main() {
  const env = process.env;
  const commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  if (env.GITHUB_EVENT_NAME !== "pull_request" ||
      commit !== env.AUTHORIZED_COMMIT || commit !== env.GITHUB_SHA) {
    throw new Error("Unauthorized release event or checkout");
  }
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], {
    cwd: root, encoding: "utf8",
  }));
  const version = metadata.packages.find(pkg => pkg.name === "windows-app").version;
  const identity = releaseIdentity(commit, version, readFileSync(join(root, "CHANGELOG.md"), "utf8"));
  const api = githubApi(env.GITHUB_REPOSITORY, env.GH_TOKEN);
  const directory = join(root, "target/packaging/windows");
  if (process.argv[2] === "prepare") {
    const candidate = await recoverCandidate(api, directory, identity, env.GITHUB_RUN_ID,
      (runId, name, destination) => execFileSync("gh", [
        "run", "download", runId, "--repo", env.GITHUB_REPOSITORY, "--name", name, "--dir", destination,
      ], { stdio: "inherit" }));
    const plan = await publicationPlan(api, identity, candidate);
    appendFileSync(env.GITHUB_OUTPUT, `plan=${plan}\n`);
    console.log(`Release ${identity.tag}: ${plan}`);
  } else if (process.argv[2] === "publish") {
    const candidate = existsSync(join(directory, manifestName)) ? readCandidate(directory, identity) : null;
    await publishRelease(api, identity, candidate, candidate && join(directory, candidate.filename));
    console.log(`Release ${identity.tag} published`);
  } else {
    throw new Error("Expected prepare or publish");
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
