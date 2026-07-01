#!/usr/bin/env node

/**
 * Test the wrapper script platform selection and fallback npm exec command.
 */

import assert from "node:assert/strict";

process.env.ACP_CLAUDE_PTY_SKIP_RUN = "1";

const {
  buildNpmExecArgs,
  getPlatformPackage,
  getPlatformPackageSpec,
} = await import("../bin/acp-extension-claude-pty.js");

const testCases = [
  { platform: "darwin", arch: "arm64", expected: "acp-extension-claude-pty-darwin-arm64" },
  { platform: "darwin", arch: "x64", expected: "acp-extension-claude-pty-darwin-x64" },
  { platform: "linux", arch: "arm64", expected: "acp-extension-claude-pty-linux-arm64" },
  { platform: "linux", arch: "x64", expected: "acp-extension-claude-pty-linux-x64" },
  { platform: "win32", arch: "arm64", expected: "acp-extension-claude-pty-win32-arm64" },
  { platform: "win32", arch: "x64", expected: "acp-extension-claude-pty-win32-x64" },
];

console.log("Testing platform detection logic...\n");

for (const testCase of testCases) {
  const result = getPlatformPackage(testCase.platform, testCase.arch);
  assert.equal(result, testCase.expected);
  console.log(`✓ ${testCase.platform}-${testCase.arch} -> ${result}`);
}

assert.throws(
  () => getPlatformPackage("freebsd", "x64"),
  /Unsupported platform: freebsd/,
);
assert.throws(
  () => getPlatformPackage("darwin", "riscv64"),
  /Unsupported architecture: riscv64 on darwin/,
);

console.log("\nTesting fallback npm exec command...\n");

const packageName = "acp-extension-claude-pty-darwin-arm64";
const packageSpec = getPlatformPackageSpec(packageName, {
  version: "1.2.3",
  optionalDependencies: {
    [packageName]: "4.5.6",
  },
});
assert.equal(packageSpec, `${packageName}@4.5.6`);

const args = buildNpmExecArgs(packageSpec, ["acp", "--debug"]);
assert.deepEqual(args, [
  "exec",
  "--yes",
  "--package",
  `${packageName}@4.5.6`,
  "--",
  "acp-extension-claude-pty",
  "acp",
  "--debug",
]);

console.log(`✓ npm ${args.join(" ")}`);

console.log("\n✓ All wrapper tests passed!");
console.log("\nCurrent platform:");
console.log(`  Platform: ${process.platform}`);
console.log(`  Arch: ${process.arch}`);
console.log(`  Package: ${getPlatformPackage()}`);
