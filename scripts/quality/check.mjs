import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const handwritten = ["crates/receiver-core", "crates/windows-app"];
const packages = ["-p", "receiver-core", "-p", "windows-app"];
const windowsTarget = "x86_64-pc-windows-msvc";
const generated = new Map([
  ["pin-glide-http", "generated/http-client"],
  ["pin-glide-wss", "generated/wss-models"],
]);
const coreDependencies = new Set(["aes-gcm", "base64", "getrandom", "hpke", "serde", "serde_json", "subtle", "zeroize"]);

function run(command, args, capture = false) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, SLINT_COMPILER_DENY_WARNINGS: "1" },
    stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${result.signal ?? result.status})`);
  }
  return result.stdout;
}

export function handwrittenSources(fileList) {
  return [...new Set(fileList.split("\0"))].filter(
    (file) =>
      handwritten.some((directory) => file.startsWith(`${directory}/`)) &&
      /\.(rs|slint)$/.test(file) &&
      !/(^|\/)(generated|vendor|third-party)\//.test(file),
  );
}

function normalizeNewlines(text) {
  return text.replace(/\r\n/g, "\n");
}

export function sourceIssues(file, source, formatSlint) {
  const text = normalizeNewlines(source);
  const lines = text === "" ? 0 : text.replace(/\n$/, "").split("\n").length;
  const issues = [];
  if (lines > 500) issues.push(`${file}: ${lines} physical lines (maximum 500)`);
  if (file.endsWith(".slint") && text !== normalizeNewlines(formatSlint(file))) {
    issues.push(`${file}: Slint format differs. Run slint-lsp format --inline "${file}".`);
  }
  return issues;
}

export function dependencyIssues(metadata) {
  const issues = [];
  const members = new Set(metadata.workspace_members);
  const locations = new Map([
    ["receiver-core", handwritten[0]],
    ["windows-app", handwritten[1]],
    ...generated,
  ]);
  for (const pkg of metadata.packages.filter((item) => members.has(item.id))) {
    const directory = locations.get(pkg.name);
    const expectedManifest = directory && resolve(metadata.workspace_root, directory, "Cargo.toml");
    if (resolve(pkg.manifest_path) !== expectedManifest) {
      issues.push(`${pkg.name}: unexpected workspace package at ${pkg.manifest_path}`);
      continue;
    }
    for (const dependency of pkg.dependencies) {
      if (pkg.name === "receiver-core") {
        if (dependency.source?.startsWith("registry+") && coreDependencies.has(dependency.name)) {
          continue;
        }
      } else if (dependency.source && !dependency.path) {
        continue;
      } else if (pkg.name === "windows-app" && dependency.path) {
        if (dependency.name === "receiver-core" || generated.has(dependency.name)) {
          const expected = resolve(metadata.workspace_root, locations.get(dependency.name));
          if (resolve(dependency.path) === expected) continue;
        }
      }
      issues.push(`${pkg.name}: forbidden dependency ${dependency.name}`);
    }
  }
  return issues;
}

function enforce(issues) {
  if (issues.length) throw new Error(issues.join("\n"));
}

function checkSource() {
  const files = handwrittenSources(
    run("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", ...handwritten], true),
  ).filter((file) => existsSync(resolve(root, file)));
  if (!files.some((file) => file.endsWith(".rs")) || !files.some((file) => file.endsWith(".slint"))) {
    throw new Error("Expected handwritten Rust and Slint sources.");
  }
  enforce(
    files.flatMap((file) =>
      sourceIssues(file, readFileSync(resolve(root, file), "utf8"), (path) =>
        run("slint-lsp", ["format", path], true),
      ),
    ),
  );
  run("cargo", ["fmt", ...packages, "--check"]);
}

function checkDependencies() {
  const metadata = run("cargo", ["metadata", "--locked", "--format-version", "1", "--no-deps"], true);
  enforce(dependencyIssues(JSON.parse(metadata)));
}

function main(mode) {
  if (mode === "source" || mode === "quality") checkSource();
  if (mode === "lint" || mode === "quality") {
    checkDependencies();
    run("cargo", [
      "clippy", "--locked", ...packages, "--all-targets",
      "--target", windowsTarget, "--no-deps", "--", "-D", "warnings",
    ]);
  }
  if (mode === "build") {
    run("cargo", ["check", "--locked", "--workspace", "--all-targets", "--target", windowsTarget]);
    run("cargo", ["build", "--locked", "-p", "windows-app", "--release", "--target", windowsTarget]);
  }
  if (mode === "service") {
    run("cargo", ["test", "--locked", "-p", "windows-app", "--lib", "--test", "pairing_confirmation", "--target", windowsTarget]);
  }
  if (mode === "core") {
    checkDependencies();
    run("cargo", [
      "clippy", "--locked", "-p", "receiver-core", "--all-targets",
      "--no-deps", "--", "-D", "warnings",
    ]);
    run("cargo", ["test", "--locked", "-p", "receiver-core"]);
  }
  if (mode === "advisories") run("cargo", ["deny", "--locked", "check", "advisories"]);
  if (!["source", "lint", "quality", "build", "service", "core", "advisories"].includes(mode)) {
    throw new Error(`Unknown quality check: ${mode}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv[2] ?? "quality");
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
