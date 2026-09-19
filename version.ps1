<#
.SYNOPSIS
  将 package.json 中的版本号同步到 tauri.conf.json 与 src-tauri/Cargo.toml、Cargo.lock；
  将 msiVersion（MSI 发布序列号）同步到 tauri.conf.json 的 bundle.windows.wix.version。

.DESCRIPTION
  版本号只需维护在仓库根目录的 package.json 中（version / msiVersion / baseVersion 字段）：
    - version：软件版本（格式 X.Y.Z 或 X.Y.Z-beta.N，如 1.0.0、1.0.0-beta.1）
    - msiVersion：Windows MSI 发布序列号（格式 X.Y.Z，第 N 次发布为 1.0.N）
    - baseVersion：目标版本线（vX.Y.Z），由 tag.ps1 用于校验 version 基线
  本脚本会写入：
    - tauri.conf.json 的 version（应用版本）
    - tauri.conf.json 的 bundle.windows.wix.version（MSI 版本，取 msiVersion）
    - src-tauri/Cargo.toml 的 [package] version（Rust 后端版本）
    - src-tauri/Cargo.lock 中根包 git-repo-sync 的 version（保持锁文件一致）

  msiVersion 独立于软件版本号的原因：MSI 版本比较只看前 3 段且不允许字母，
  无法表达 beta 语义。用发布次数绑定序列号（1.0.N）即可单调递增：
  历史 beta 的 MSI 版本为 1.0.0.x（比较值 1.0.0），1.0.N 高于它，旧版可直接覆盖升级。
  发布前 tag.ps1 会检查各版本文件与 package.json 的一致性。
  之后可运行 tag.ps1 按同一版本打 tag。

.EXAMPLE
  .\version.ps1        # 按 package.json 同步各版本位置
#>

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

$package = Get-Content (Join-Path $RepoRoot 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $package.version
if (-not $version) {
    throw 'package.json 中缺少 version 字段。'
}
if ($version -notmatch '^\d+\.\d+\.\d+(-beta\.\d+)?$') {
    throw "package.json 的 version 格式不正确：$version（应为 X.Y.Z 或 X.Y.Z-beta.N，例如 1.0.0、1.0.0-beta.1）"
}

# MSI 发布序列号：独立于软件版本，第 N 次发布为 1.0.N
$msiVersion = $package.msiVersion
if (-not $msiVersion) {
    throw 'package.json 中缺少 msiVersion 字段。'
}
if ($msiVersion -notmatch '^\d+\.\d+\.\d+$') {
    throw "package.json 的 msiVersion 格式不正确：$msiVersion（应为数字三段 X.Y.Z，例如 1.0.1）"
}

$nodeScript = @'
const fs = require('fs');
const ver = process.argv[1], wix = process.argv[2];
const p = JSON.parse(fs.readFileSync('package.json', 'utf8'));
p.version = ver;
fs.writeFileSync('package.json', JSON.stringify(p, null, 2) + String.fromCharCode(10));
const t = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
t.version = ver;
t.bundle.windows.wix.version = wix;
fs.writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(t, null, 2) + String.fromCharCode(10));
'@
node -e $nodeScript $version $msiVersion
if ($LASTEXITCODE -ne 0) { throw '同步版本到 package.json / tauri.conf.json 失败' }

# Cargo.toml 的 [package] version 与 Cargo.lock 中根包 git-repo-sync 的 version。
# Cargo.lock 中其他包的 version 行不能动，因此按「name 行 + 紧跟的 version 行」定位。
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$cargoFiles = @(
    (Join-Path $RepoRoot 'src-tauri\Cargo.toml'),
    (Join-Path $RepoRoot 'src-tauri\Cargo.lock')
)
foreach ($path in $cargoFiles) {
    if (-not (Test-Path $path)) { throw "缺少文件：$path" }
    $content = [System.IO.File]::ReadAllText($path)
    $pattern = if ($path -like '*Cargo.toml') {
        '(?m)(^version = ")[^"]*"'
    } else {
        '(?m)(name = "git-repo-sync"\r?\nversion = ")[^"]*"'
    }
    if (-not [regex]::IsMatch($content, $pattern)) {
        throw "未找到待同步的版本行：$path"
    }
    $new = [regex]::Replace($content, $pattern, ('${1}' + $version + '"'))
    [System.IO.File]::WriteAllText($path, $new, $utf8NoBom)
}

Write-Host "已同步 package.json / tauri.conf.json / Cargo.toml / Cargo.lock（version=$version，wix.version=$msiVersion）"
