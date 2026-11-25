import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const manifest = "crates/windows-app/Cargo.toml";
const lockfile = "Cargo.lock";
const changelog = "CHANGELOG.md";
const releasePaths = new Set([manifest, lockfile, changelog]);
const releaseBranch = "auto-release";
const releaseRef = `refs/heads/${releaseBranch}`;
const trackingRef = `refs/remotes/origin/${releaseBranch}`;
const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function requireVersion(version) {
  if (!stableVersion.test(version)) throw new Error(`Expected stable Cargo version, found ${version}`);
  if (version.split(".").some(part => BigInt(part) > 65535n)) {
    throw new Error(`Version ${version} must fit the Windows version resource (0–65535 per component)`);
  }
  return version;
}

function compareVersions(left, right) {
  const leftParts = left.split(".").map(BigInt);
  const rightParts = right.split(".").map(BigInt);
  for (const [index, part] of leftParts.entries()) {
    if (part !== rightParts[index]) return part > rightParts[index] ? 1 : -1;
  }
  return 0;
}

function packageSection(text, file) {
  const header = file === manifest ? "[package]" : "[[package]]";
  const sections = (text ?? "").split(/(?=^\[)/m);
  const matching = sections.filter(section =>
    section.startsWith(`${header}\n`) || section.startsWith(`${header}\r\n`))
    .filter(section => /^name = "windows-app"\r?$/m.test(section));
  if (matching.length !== 1) throw new Error(`Expected one windows-app package in ${file}`);
  const section = matching[0];
  const version = /^version = "([^"]+)"(?=\r?$)/m.exec(section)?.[1];
  return { section, version: requireVersion(version) };
}

function updateVersion(text, file, version) {
  const { section } = packageSection(text, file);
  return text.replace(section, () => section.replace(/^version = "[^"]+"(?=\r?$)/m, `version = "${version}"`));
}

function renderChangelog(existing, version, entries) {
  const [preamble, ...sections] = (existing ?? "# Changelog\n")
    .replace(/\r\n/g, "\n").trimEnd().split(/\n+(?=## )/);
  const kept = sections.filter(section => section.split("\n")[0] !== `## ${version}`);
  return [preamble, `## ${version}`, entries.map(entry => `- ${entry}`).join("\n"), ...kept]
    .join("\n\n") + "\n";
}

function releasePulls(gh, state) {
  return JSON.parse(gh(["pr", "list", "--base", "main", "--head", releaseBranch,
    "--state", state, "--limit", "100", "--json", "number,mergeCommit,isCrossRepository"]))
    .filter(pr => !pr.isCrossRepository);
}

function patchAfter(version) {
  return version.replace(/\d+$/, patch => String(BigInt(patch) + 1n));
}

function selectVersion(latest, current) {
  const minimum = latest ? patchAfter(latest) : "0.1.0";
  if (compareVersions(current, minimum) < 0) return minimum;
  return current;
}

function releaseBaseline(git, merged, head) {
  const tags = git("tag", "--list", "v*").split("\n").map(tag => tag.slice(1))
    .filter(version => stableVersion.test(version)).sort(compareVersions);
  const latest = tags.at(-1);
  const base = latest ? `v${latest}` : null;
  const commit = base ? git("rev-parse", `${base}^{commit}`) : null;
  if (commit && git("merge-base", commit, head) !== commit) {
    throw new Error("Latest stable release is not an ancestor of main");
  }
  const pending = git("log", "--first-parent", "--format=%H", base ? `${base}..HEAD` : "HEAD")
    .split("\n").find(sha => merged.has(sha));
  if (!pending) return { base, commit, version: latest };
  const version = packageSection(git("show", `${pending}:${manifest}`), manifest).version;
  if (latest && compareVersions(version, latest) <= 0) {
    throw new Error("Authorized release version must exceed the latest stable tag");
  }
  return { base: pending, commit: pending, version };
}

export function checkVersion(read) {
  const product = packageSection(read(manifest), manifest).version;
  if (packageSection(read(lockfile), lockfile).version !== product) {
    throw new Error("windows-app manifest and lockfile versions must agree");
  }
}

function pushEvent(git, before, after) {
  if (![before, after].every(sha => /^[0-9a-f]{40}$/.test(sha ?? ""))) {
    throw new Error("PUSH_BEFORE and PUSH_AFTER must be the push event SHAs");
  }
  if (git("branch", "--show-current") !== "main" || git("status", "--porcelain")) {
    throw new Error("Release proposals require a clean main checkout");
  }
  const head = git("rev-parse", "HEAD");
  if (git("merge-base", after, head) !== after) {
    throw new Error("Push event is not an ancestor of the current main checkout");
  }
  const paths = (/^0+$/.test(before)
    ? git("ls-tree", "-r", "--name-only", "-z", after)
    : git("diff", "--no-renames", "--name-only", "-z", before, after)).split("\0").filter(Boolean);
  return { head, paths };
}

export function updateProposal({ before, after, git, gh, read, write }) {
  const { head, paths } = pushEvent(git, before, after);
  const merged = new Set(releasePulls(gh, "merged").map(pr => pr.mergeCommit?.oid).filter(Boolean));
  const baseline = releaseBaseline(git, merged, head);
  const alreadyAuthorized = baseline.commit && git("merge-base", after, baseline.commit) === after;
  const eligible = !alreadyAuthorized && paths.some(path => !path.startsWith(".github/")) &&
    !(paths.every(path => releasePaths.has(path)) && merged.has(after));
  const open = releasePulls(gh, "open");
  if (open.length > 1) throw new Error("Expected at most one open Release PR");
  if (!eligible && !open.length) {
    console.log("No release-eligible changes; skipping release preparation.");
    return;
  }

  const { base } = baseline;
  const current = packageSection(read(manifest), manifest).version;
  const history = read(changelog);
  let version = selectVersion(baseline.version, current);
  const previousHead = git("for-each-ref", "--format=%(objectname)", trackingRef);
  if (open.length) {
    if (!previousHead) throw new Error("Open Release PR has no checked-out branch reference");
    const proposed = packageSection(git("show", `${previousHead}:${manifest}`), manifest).version;
    if (compareVersions(proposed, version) > 0) version = proposed;
  }
  requireVersion(version);
  const entries = git("log", "--no-merges", "--pretty=format:%s", base ? `${base}..HEAD` : "HEAD")
    .split("\n").filter(Boolean);
  if (!entries.length) throw new Error("No commits to include in the release notes");
  const title = `chore: propose release v${version}`;
  const body = [
    `## Proposed release ${version}`, "", `Changelog base: ${base ?? "the repository root"}.`,
    "", "### Changelog", "", ...entries.map(entry => `- ${entry}`),
    "", "---", "",
    "This pull request is refreshed on pushes to `main`. Only release-eligible pushes update the branch.",
    `Merging it authorizes production publication of the Windows Receiver installer and tag \`v${version}\`.`,
    "Review the version, changelog and independent CI checks before merging manually.",
  ].join("\n");

  if (eligible) {
    const manifestText = updateVersion(read(manifest), manifest, version);
    const lockText = updateVersion(read(lockfile), lockfile, version);
    write(manifest, manifestText);
    write(lockfile, lockText);
    write(changelog, renderChangelog(history, version, entries));
    git("add", "--", manifest, lockfile, changelog);
    if (!git("diff", "--cached", "--name-only")) throw new Error("No release proposal changes to commit");
    git("-c", "user.name=github-actions[bot]", "-c",
      "user.email=41898282+github-actions[bot]@users.noreply.github.com", "commit", "-m", title);
    git("push", `--force-with-lease=${releaseRef}:${previousHead}`, "origin", `HEAD:${releaseRef}`);
  }

  gh(open.length
    ? ["pr", "edit", String(open[0].number), "--title", title, "--body-file", "-"]
    : ["pr", "create", "--base", "main", "--head", releaseBranch, "--title", title, "--body-file", "-"],
  body);
}

function run(command, args, input) {
  const result = spawnSync(command, args, { encoding: "utf8", input, timeout: 300_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr.trim()}`);
  return result.stdout.trim();
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const read = file => {
      try {
        return readFileSync(file, "utf8");
      } catch (error) {
        if (error.code === "ENOENT") return null;
        throw error;
      }
    };
    if (process.argv[2] === "check-version") checkVersion(read);
    else updateProposal({
      before: process.env.PUSH_BEFORE,
      after: process.env.PUSH_AFTER,
      git: (...args) => run("git", args),
      gh: (args, body) => run("gh", args, body),
      read,
      write: writeFileSync,
    });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
