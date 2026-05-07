#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const path = require("path");

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@gitcortex/gcx-darwin-arm64",
  "darwin-x64":   "@gitcortex/gcx-darwin-x64",
  "linux-x64":    "@gitcortex/gcx-linux-x64",
  "linux-arm64":  "@gitcortex/gcx-linux-arm64",
};

function findBinary() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];
  if (!pkg) {
    throw new Error(
      `gcx: unsupported platform ${key}.\n` +
      `Supported: ${Object.keys(PLATFORM_PACKAGES).join(", ")}\n` +
      `Install from source: cargo install gitcortex`
    );
  }
  try {
    return require.resolve(`${pkg}/bin/gcx`);
  } catch {
    throw new Error(
      `gcx: platform package ${pkg} not found.\n` +
      `Try reinstalling: npm install -g gitcortex`
    );
  }
}

const binary = findBinary();
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
