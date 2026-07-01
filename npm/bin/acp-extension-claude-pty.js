#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

const COMMAND_NAME = "acp-extension-claude-pty";

function getPackageJson() {
  const packageJsonPath = fileURLToPath(new URL("../package.json", import.meta.url));
  return JSON.parse(readFileSync(packageJsonPath, "utf8"));
}

export function getPlatformPackage(platform = process.platform, arch = process.arch) {
  const baseName = COMMAND_NAME;

  const platformMap = {
    darwin: {
      arm64: `${baseName}-darwin-arm64`,
      x64: `${baseName}-darwin-x64`,
    },
    linux: {
      arm64: `${baseName}-linux-arm64`,
      x64: `${baseName}-linux-x64`,
    },
    win32: {
      arm64: `${baseName}-win32-arm64`,
      x64: `${baseName}-win32-x64`,
    },
  };

  const packages = platformMap[platform];
  if (!packages) {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  const packageName = packages[arch];
  if (!packageName) {
    throw new Error(`Unsupported architecture: ${arch} on ${platform}`);
  }

  return packageName;
}

function getBinaryName(platform = process.platform) {
  return platform === "win32" ? `${COMMAND_NAME}.exe` : COMMAND_NAME;
}

export function getPlatformPackageSpec(packageName, packageJson = getPackageJson()) {
  const version = packageJson.optionalDependencies?.[packageName] ?? packageJson.version;
  return `${packageName}@${version}`;
}

function getNpmExecCommand() {
  if (process.env.npm_execpath) {
    return {
      command: process.execPath,
      argsPrefix: [process.env.npm_execpath],
    };
  }

  return {
    command: process.platform === "win32" ? "npm.cmd" : "npm",
    argsPrefix: [],
  };
}

function resolveBinaryPath(packageName = getPlatformPackage()) {
  const binaryName = getBinaryName();

  try {
    const binaryPath = fileURLToPath(
      import.meta.resolve(`${packageName}/bin/${binaryName}`),
    );

    if (existsSync(binaryPath)) {
      return binaryPath;
    }
  } catch (e) {
    if (process.env.ACP_CLAUDE_PTY_DEBUG) {
      console.error(`Error resolving package: ${e}`);
    }
  }

  return undefined;
}

function ensureExecutable(binaryPath) {
  if (process.platform === "win32") return;

  try {
    const st = statSync(binaryPath);
    if ((st.mode & 0o111) === 0) {
      chmodSync(binaryPath, st.mode | 0o111);
    }
  } catch {
    // Best-effort: spawnSync will report the real failure if execution is blocked.
  }
}

export function buildNpmExecArgs(packageSpec, commandArgs = []) {
  return [
    "exec",
    "--yes",
    "--package",
    packageSpec,
    "--",
    COMMAND_NAME,
    ...commandArgs,
  ];
}

function runViaNpmExec(packageName, commandArgs) {
  const packageSpec = getPlatformPackageSpec(packageName);
  const { command, argsPrefix } = getNpmExecCommand();
  const result = spawnSync(command, [...argsPrefix, ...buildNpmExecArgs(packageSpec, commandArgs)], {
    stdio: "inherit",
    windowsHide: true,
  });

  if (result.error) {
    console.error(`Failed to install and execute ${packageSpec}:`, result.error);
    process.exit(1);
  }

  process.exit(result.status || 0);
}

function run() {
  const packageName = getPlatformPackage();
  const binaryPath = resolveBinaryPath(packageName);

  if (!binaryPath) {
    runViaNpmExec(packageName, process.argv.slice(2));
    return;
  }

  ensureExecutable(binaryPath);
  const result = spawnSync(binaryPath, process.argv.slice(2), {
    stdio: "inherit",
    windowsHide: true,
  });

  if (result.error) {
    console.error(`Failed to execute ${binaryPath}:`, result.error);
    process.exit(1);
  }

  process.exit(result.status || 0);
}

if (process.env.ACP_CLAUDE_PTY_SKIP_RUN !== "1") {
  try {
    run();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
