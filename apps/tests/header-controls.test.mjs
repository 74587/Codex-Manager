import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const headerPath = path.join(
  testDir,
  "..",
  "src",
  "components",
  "layout",
  "header.tsx",
);

test("管理员仪表盘保留服务开关和语言选择", async () => {
  const source = await fs.readFile(headerPath, "utf8");

  assert.match(source, /<Switch[\s\S]*?onCheckedChange=\{handleToggleService\}/);
  assert.match(
    source,
    /<LanguageSwitcher[\s\S]*?compact[\s\S]*?triggerClassName="w-10 min-w-10 gap-1 px-0[\s\S]*?sm:w-\[124px\][\s\S]*?sm:gap-2/,
  );
  assert.match(source, /<DisclaimerTicker compact \/>/);
  assert.match(source, /v\{serviceStatus\.version\}/);
  assert.doesNotMatch(source, /!isCommandCenter\s*\?\s*\(\s*<Switch/);
  assert.doesNotMatch(source, /!isCommandCenter\s*\?\s*<LanguageSwitcher/);
});
