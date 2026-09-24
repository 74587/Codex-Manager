import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "codex-profile-client.ts",
);

async function loadClientModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-codex-profile-client-"),
  );
  const tempFile = path.join(tempDir, "codex-profile-client.mjs");
  await fs.writeFile(
    path.join(tempDir, "transport.mjs"),
    "export async function invoke(command, params) { globalThis.__codexProfileInvokeCalls ??= []; globalThis.__codexProfileInvokeCalls.push({ command, params }); return globalThis.__codexProfileInvokeResult ?? {}; }\nexport function withAddr(value = {}) { return value; }\n",
    "utf8",
  );
  await fs.writeFile(
    tempFile,
    compiled.outputText.replace("./transport", "./transport.mjs"),
    "utf8",
  );
  return import(pathToFileURL(tempFile).href);
}

const client = await loadClientModule();

test("normalizeCodexProfileStatus reads the managed catalog state", () => {
  assert.equal(
    client.normalizeCodexProfileStatus({
      mode: "gateway",
      profile_writable: true,
      managed_catalog_active: true,
    }).managedCatalogActive,
    true,
  );
  assert.equal(
    client.normalizeCodexProfileStatus({
      mode: "gateway",
      profileWritable: true,
      managedCatalogActive: false,
    }).managedCatalogActive,
    false,
  );
});

test("applyModels uses the standalone command without gateway credentials", async () => {
  globalThis.__codexProfileInvokeCalls = [];
  globalThis.__codexProfileInvokeResult = {
    codex_home: "/srv/codex",
    mode: "gateway",
    profile_writable: true,
    managed_catalog_active: true,
  };

  const status = await client.codexProfileClient.applyModels({
    codexHome: "/srv/codex",
    modelSlugs: ["gpt-5.6-sol", "gpt-image-2"],
  });

  assert.deepEqual(globalThis.__codexProfileInvokeCalls, [
    {
      command: "service_codex_profile_apply_models",
      params: {
        codexHome: "/srv/codex",
        modelSlugs: ["gpt-5.6-sol", "gpt-image-2"],
        reloadAfterSwitch: false,
      },
    },
  ]);
  assert.equal(status.managedCatalogActive, true);
  assert.equal(status.profileWritable, true);
});
