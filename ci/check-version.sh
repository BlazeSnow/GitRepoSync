#!/usr/bin/env bash
# CI 校验：package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml 与
# src-tauri/Cargo.lock（根包 git-repo-sync）的版本必须与期望版本一致；
# Windows MSI 版本（bundle.windows.wix.version）必须与 package.json 的 msiVersion 一致。
# msiVersion 独立于软件版本（MSI 比较只看前 3 段且不允许字母），由发布序列号控制。
# 用法：ci/check-version.sh <期望版本>，如 bash ci/check-version.sh 1.0.0-beta.1
set -euo pipefail

want="$1"

node -e '
  const fs = require("fs");
  const want = process.argv[1];
  const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
  const tauri = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
  const wixVersion = tauri.bundle?.windows?.wix?.version ?? null;
  const msiVersion = pkg.msiVersion;
  const cargoToml = fs.readFileSync("src-tauri/Cargo.toml", "utf8").replace(/^\uFEFF/, "");
  const cargoTomlVersion = (cargoToml.match(/^version = "([^"]+)"/m) ?? [])[1];
  const lock = fs.readFileSync("src-tauri/Cargo.lock", "utf8");
  const lockVersion = (lock.match(/\[\[package\]\]\r?\nname = "git-repo-sync"\r?\nversion = "([^"]+)"/) ?? [])[1];
  const files = {
    "package.json": pkg.version,
    "src-tauri/tauri.conf.json": tauri.version,
    "src-tauri/Cargo.toml": cargoTomlVersion,
    "src-tauri/Cargo.lock (git-repo-sync)": lockVersion,
  };
  let ok = true;
  if (!pkg.baseVersion || !/^v\d+\.\d+\.\d+$/.test(pkg.baseVersion)) {
    console.error(`package.json 的 baseVersion 缺失或格式不正确：${pkg.baseVersion}（应为 vX.Y.Z，如 v1.0.0）`);
    ok = false;
  }
  for (const [f, v] of Object.entries(files)) {
    if (v !== want) {
      console.error(`版本不一致：${f} 为 ${v}，期望 ${want}`);
      ok = false;
    }
  }
  if (wixVersion !== msiVersion) {
    console.error(`wix.version 应为 ${msiVersion}（package.json 的 msiVersion 发布序列号），当前为 ${wixVersion ?? "未设置"}`);
    ok = false;
  }
  if (!ok) process.exit(1);
  console.log(`版本一致：${want}，wix.version ${wixVersion}`);
' "$want"
