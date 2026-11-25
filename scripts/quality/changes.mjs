import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";

export function classifyChanges(paths) {
  const matches = (pattern) => paths.some((path) => pattern.test(path));
  const dependency = matches(/^(?:(?:crates|generated)\/[^/]+\/)?Cargo\.toml$|^Cargo\.lock$|^rust-toolchain(\.toml)?$|^\.cargo\//);
  const quality = matches(/^scripts\/quality\/|^\.github\/(workflows\/ci\.yml$|actions\/)/);
  const packaging = dependency || quality || matches(
    /^packaging\/windows\/(?!.*\.md$)|^\.github\/workflows\/(development-packaging|production-release)\.yml$|^crates\/windows-app\/(build\.rs$|ui\/assets\/)/,
  );
  return {
    windows: packaging || matches(/^crates\/(receiver-core|windows-app)\/|^generated\/|^(clippy|rustfmt)\.toml$|^docs\/prototypes\/ui-tool\/data\/messages\.js$/),
    packaging,
    release: quality || matches(/^scripts\/release\/|^\.github\/workflows\/auto-release-pr\.yml$|^packaging\/windows\/production-publication\.mjs$/),
    advisories: dependency || quality || matches(/^deny\.toml$/),
    contracts: matches(/^scripts\/contracts\/|^generated\/|^Cargo\.lock$|^\.github\/workflows\/contracts\.yml$/),
  };
}

export function classifyRange(base, head, cwd = process.cwd()) {
  try {
    if (!base || !head) throw new Error("Missing comparison range");
    const paths = execFileSync("git", [
      "diff", "--name-only", "--no-renames", "-z", base, head, "--",
    ], { cwd, encoding: "utf8", maxBuffer: 16 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });
    return classifyChanges(paths.split("\0"));
  } catch (error) {
    console.error(`Cannot compare changed paths; running all checks: ${error.message}`);
    return { windows: true, packaging: true, release: true, advisories: true, contracts: true };
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  for (const [name, enabled] of Object.entries(classifyRange(process.argv[2], process.argv[3]))) {
    console.log(`${name}=${enabled}`);
  }
}
