import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync,
} from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { Parser } from "@asyncapi/parser";
import {
  AsyncAPIInputProcessor, InputMetaModel, JsonSchemaInputProcessor, RustFileGenerator,
  defaultRustRenderCompleteModelOptions,
} from "@asyncapi/modelina";

const root = fileURLToPath(new URL("../../", import.meta.url));
const pins = JSON.parse(readFileSync(new URL("service.lock.json", import.meta.url)));
const dependencies = JSON.parse(readFileSync(new URL("package-lock.json", import.meta.url)));
const mode = process.argv[2];
assert.ok(["--check", "--write"].includes(mode), "Use --check or --write");
assert.equal(dependencies.packages["node_modules/@asyncapi/parser"].version, pins.generators.parser);
assert.equal(dependencies.packages["node_modules/@asyncapi/modelina"].version, pins.generators.modelina);

function verify(path, expected) {
  const actual = createHash("sha256").update(readFileSync(path)).digest("hex");
  assert.equal(actual, expected, `SHA-256 mismatch for ${path}`);
}

function filesIn(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(directory, join(entry.parentPath, entry.name)))
    .sort();
}

function syncCrate(source, destination) {
  const expected = filesIn(source).filter((file) => file === "Cargo.toml" || file.endsWith(".rs"));
  const existing = filesIn(destination);
  if (mode === "--check") {
    assert.deepEqual(existing, expected, `Generated file list drifted in ${destination}`);
  }
  for (const file of expected) {
    const contents = readFileSync(join(source, file), "utf8").replace(/[ \t\r]+$/gm, "").trimEnd() + "\n";
    const output = join(destination, file);
    if (mode === "--check") {
      assert.ok(readFileSync(output, "utf8") === contents, `Generated file drifted: ${output}`);
    } else {
      mkdirSync(dirname(output), { recursive: true });
      writeFileSync(output, contents);
    }
  }
  if (mode === "--write") {
    for (const file of existing.filter((file) => !expected.includes(file))) {
      rmSync(join(destination, file));
    }
  }
}

const scratch = join(root, "target");
mkdirSync(scratch, { recursive: true });
const stage = mkdtempSync(join(scratch, "service-contracts-"));
try {
  const commit = execFileSync("gh", [
    "api", `repos/${pins.repository}/commits/${pins.tag}`, "--jq", ".sha",
  ], { encoding: "utf8" }).trim();
  assert.equal(commit, pins.commit, `Service tag ${pins.tag} moved`);
  execFileSync("gh", [
    "release", "download", pins.tag, "-R", pins.repository,
    ...Object.keys(pins.assets).flatMap((asset) => ["--pattern", asset]),
    "--dir", stage,
  ], { stdio: "inherit" });
  for (const [asset, hash] of Object.entries(pins.assets)) verify(join(stage, asset), hash);

  const { version, sha256 } = pins.generators.openapi;
  const jar = join(stage, "openapi-generator-cli.jar");
  const response = await fetch(
    `https://repo.maven.apache.org/maven2/org/openapitools/openapi-generator-cli/${version}/openapi-generator-cli-${version}.jar`,
    { signal: AbortSignal.timeout(120_000) },
  );
  assert.ok(response.ok, `OpenAPI Generator download failed: HTTP ${response.status}`);
  writeFileSync(jar, Buffer.from(await response.arrayBuffer()));
  verify(jar, sha256);

  const http = join(stage, "http-client");
  const wss = join(stage, "wss-models");
  const packageVersion = pins.tag.slice(1);
  execFileSync("java", [
    "-jar", jar, "generate", "-i", join(stage, "openapi.yaml"), "-g", "rust", "-o", http,
    "--global-property", "apiDocs=false,modelDocs=false,apiTests=false,modelTests=false",
    "--additional-properties", [
      "packageName=pin-glide-http", `packageVersion=${packageVersion}`, "library=reqwest",
      "supportAsync=true", "supportMultipleResponses=true", "reqwestDefaultFeatures=native-tls",
      "hideGenerationTimestamp=true", "useChrono=false",
    ].join(","),
  ], { stdio: "inherit" });

  const { document, diagnostics } = await new Parser().parse(readFileSync(join(stage, "asyncapi.yaml"), "utf8"));
  assert.ok(document && !diagnostics.some(({ severity }) => severity === 0), JSON.stringify(diagnostics));
  const payloads = new InputMetaModel();
  for (const message of document.components().messages().all()) {
    const schema = AsyncAPIInputProcessor.convertToInternalSchema(message.payload());
    const model = JsonSchemaInputProcessor.convertSchemaToMetaModel(schema, {});
    assert.ok(model.name && !payloads.models[model.name], `Expected a unique named payload for ${message.id()}`);
    payloads.models[model.name] = model;
  }
  assert.ok(Object.keys(payloads.models).length, "Expected WSS payloads");
  const nullableErrors = ({ model, field }) =>
    model.name === "BusinessError" && field.unconstrainedPropertyName === "errors";
  const preserveNullableErrors = {
    struct: {
      field(args) {
        return nullableErrors(args)
          ? `pub errors: Option<Option<${args.field.property.type}>>,`
          : args.content;
      },
      fieldMacro(args) {
        return nullableErrors(args)
          ? '#[serde(rename="errors", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]'
          : args.content;
      },
    },
    package: {
      manifest({ content }) {
        return content.replace("[dev-dependencies]", 'serde_with = { version = "^3.8", default-features = false }\n\n[dev-dependencies]');
      },
    },
  };
  const defaults = defaultRustRenderCompleteModelOptions;
  await new RustFileGenerator({ presets: [preserveNullableErrors] }).generateToPackage(payloads, wss, {
    ...defaults,
    package: { ...defaults.package, packageName: "pin-glide-wss", packageVersion, edition: "2024" },
  }, true);

  syncCrate(http, join(root, "generated", "http-client"));
  syncCrate(wss, join(root, "generated", "wss-models"));
  console.log(`Service ${pins.tag} contracts verified and generated crates ${mode === "--check" ? "match" : "updated"}.`);
} finally {
  assert.equal(dirname(stage), scratch);
  rmSync(stage, { recursive: true, force: true });
}
