#!/usr/bin/env node
const { spawnSync } = require("child_process");
const os = require("os");
const path = require("path");

const PLATFORMS = {
  "darwin-arm64": "roam-tui-darwin-arm64",
  "darwin-x64": "roam-tui-darwin-x64",
  "linux-x64": "roam-tui-linux-x64",
  "linux-arm64": "roam-tui-linux-arm64",
  "win32-x64": "roam-tui-win32-x64",
};

const key = `${os.platform()}-${os.arch()}`;
const pkg = PLATFORMS[key];

if (!pkg) {
  console.error(`Unsupported platform: ${key}`);
  process.exit(1);
}

const ext = os.platform() === "win32" ? ".exe" : "";
let binPath;
try {
  const pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
  binPath = path.join(pkgDir, "bin", `roam${ext}`);
} catch {
  console.error(
    `Package ${pkg} not installed. Your platform (${key}) may not be supported.`
  );
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

process.exit(result.status ?? 1);
