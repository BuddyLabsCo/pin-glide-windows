import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const tools = [
  {
    repository: "slint-ui/slint",
    tag: "v1.19.0",
    asset: "slint-lsp-x86_64-pc-windows-msvc.zip",
    executable: "slint-lsp.exe",
    sha256: "332eb0e37c589e4fa689c7d150012eb82c217886c165c79e95b5b3928e6d8ea9",
  },
  {
    repository: "EmbarkStudios/cargo-deny",
    tag: "0.22.0",
    asset: "cargo-deny-0.22.0-x86_64-pc-windows-msvc.tar.gz",
    executable: "cargo-deny.exe",
    sha256: "a85b270b4aa28633d2cb2be97fccd4518ccb783014914e99463e75468cb3183c",
  },
];

const directory = join(root, "target", "quality-tools");
const selected = process.argv.slice(2);
if (selected.some((name) => !tools.some((tool) => tool.executable === `${name}.exe`))) {
  throw new Error(`Unknown quality tool: ${selected.join(", ")}`);
}
for (const tool of tools.filter((tool) => !selected.length || selected.includes(tool.executable.slice(0, -4)))) {
  const destination = join(directory, `${tool.executable}-${tool.tag}`);
  mkdirSync(destination, { recursive: true });
  const url = `https://github.com/${tool.repository}/releases/download/${tool.tag}/${tool.asset}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (createHash("sha256").update(bytes).digest("hex") !== tool.sha256) {
    throw new Error(`${tool.asset}: SHA-256 mismatch`);
  }
  const archive = join(destination, tool.asset);
  writeFileSync(archive, bytes);
  const result = spawnSync("tar", ["-xf", archive, "-C", destination], { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${tool.asset}: tar exited with ${result.status}`);
  const executables = readdirSync(destination, { recursive: true, withFileTypes: true }).filter(
    (entry) => entry.isFile() && entry.name === tool.executable,
  );
  if (executables.length !== 1) throw new Error(`${tool.asset}: expected exactly one ${tool.executable}`);
  const bin = executables[0].parentPath;
  if (process.env.GITHUB_PATH) appendFileSync(process.env.GITHUB_PATH, `${bin}\n`, "utf8");
  console.log(bin);
}
