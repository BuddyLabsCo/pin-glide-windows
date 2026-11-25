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
const releaseSubject = /^chore: propose release v\d+\.\d+\.\d+(?: \(#\d+\))?$/;

function requireVersion(version) {
  if (!stableVersion.test(version)) throw new Error(`Expected stable Cargo version, found ${version}`);
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

function selectVersion(latest, current, history) {
  const minimum = latest ? patchAfter(latest) : "0.1.0";
  if (compareVersions(current, minimum) < 0) return minimum;
  // A merged proposal already owns its version even before publication creates the tag.
  if ((history ?? "").split(/\r?\n/).includes(`## ${current}`)) return patchAfter(current);
  return current;
}

function releaseEligible(paths, git, gh) {
  if (!paths.some(path => !path.startsWith(".github/"))) return false;
  if (!paths.every(path => releasePaths.has(path))) return true;
  const head = git("rev-parse", "HEAD");
  return !releaseSubject.test(git("log", "-1", "--pretty=%s", "HEAD")) &&
    !releasePulls(gh, "merged").some(pr => pr.mergeCommit?.oid === head);
}

export function updateProposal({ before, git, gh, read, write }) {
  if (!/^[0-9a-f]{40}$/.test(before ?? "")) throw new Error("PUSH_BEFORE must be the push event before SHA");
  if (git("branch", "--show-current") !== "main" || git("status", "--porcelain")) {
    throw new Error("Release proposals require a clean main checkout");
  }
  const paths = (/^0+$/.test(before)
    ? git("ls-tree", "-r", "--name-only", "-z", "HEAD")
    : git("diff", "--no-renames", "--name-only", "-z", before, "HEAD")).split("\0").filter(Boolean);
  const eligible = releaseEligible(paths, git, gh);
  const open = releasePulls(gh, "open");
  if (open.length > 1) throw new Error("Expected at most one open Release PR");
  if (!eligible && !open.length) {
    console.log("No release-eligible changes; skipping release preparation.");
    return;
  }

  const tags = git("tag", "--list", "v*").split("\n").map(tag => tag.slice(1))
    .filter(version => stableVersion.test(version)).sort(compareVersions);
  const latest = tags.at(-1);
  const base = latest ? `v${latest}` : null;
  const current = packageSection(read(manifest), manifest).version;
  const history = read(changelog);
  let version = selectVersion(latest, current, history);
  const previousHead = git("for-each-ref", "--format=%(objectname)", trackingRef);
  if (open.length) {
    if (!previousHead) throw new Error("Open Release PR has no checked-out branch reference");
    const proposed = packageSection(git("show", `${previousHead}:${manifest}`), manifest).version;
    if (compareVersions(proposed, version) > 0) version = proposed;
  }
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
    updateProposal({
      before: process.env.PUSH_BEFORE,
      git: (...args) => run("git", args),
      gh: (args, body) => run("gh", args, body),
      read: file => {
        try {
          return readFileSync(file, "utf8");
        } catch (error) {
          if (error.code === "ENOENT") return null;
          throw error;
        }
      },
      write: writeFileSync,
    });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
