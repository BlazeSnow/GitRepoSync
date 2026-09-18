<#
.SYNOPSIS
  检查项目版本文件与 package.json 一致、开发者也已手动更新版本后，创建并推送 git tag 发布。

.DESCRIPTION
  仓库的 Release 工作流在推送 v* 格式的 tag 时自动打包并创建 Release（vX.Y.Z-beta.N
  格式的 tag 会标为 Prerelease）。
  执行流程：
    1. 检查项目版本文件是否符合 package.json（tauri.conf.json / Cargo.toml / Cargo.lock 的
       version 必须一致，wix.version 必须等于 msiVersion，version 必须符合 baseVersion 基线）；
       检查不通过则拒绝继续，提示先运行 version.ps1 同步。
    2. 提示开发者确认已手动修改本次发布版本（version / msiVersion）——tag.ps1 不做任何推断
       和修改，tag 就是 v + package.json 的 version。
    3. 检查本地与远程均不存在该 tag，确认后创建并推送。
  也可以用 -Tag 显式指定版本号（跳过一致性检查与版本确认）。

.PARAMETER Tag
  要打的版本号，格式 vX.Y.Z 或 vX.Y.Z-beta.N，例如 v1.0.0、v1.0.0-beta.1。
  缺省时使用 v + package.json 的 version。

.PARAMETER NoPush
  只创建 tag，不推送到远程（不会触发任何工作流）。

.EXAMPLE
  .\tag.ps1                       # 检查一致性并确认版本后打 tag（v + package.json.version）
  .\tag.ps1 -Tag v1.0.0-beta.2    # 显式指定版本号（跳过检查）
  .\tag.ps1 -NoPush               # 只创建本地 tag
#>

[CmdletBinding()]
param(
    [string]$Tag,
    [switch]$NoPush
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$PackagePath = Join-Path $RepoRoot 'package.json'
Push-Location $RepoRoot
try {
    # 0. 读取 package.json
    $pkg = Get-Content $PackagePath -Raw -Encoding UTF8 | ConvertFrom-Json
    $base = $pkg.baseVersion
    if (-not $base -or $base -notmatch '^v\d+\.\d+\.\d+$') {
        throw "package.json 的 baseVersion 缺失或格式不正确：$base（应为 vX.Y.Z，例如 v1.0.0）"
    }
    $ver3 = $base.Substring(1)

    # 1. 检查项目版本文件是否符合 package.json（-Tag 显式指定时跳过）
    if (-not $Tag) {
        $issues = @()

        # version 必须符合 baseVersion 基线（1.0.0 或 1.0.0-beta.N）
        if ($pkg.version -notmatch "^$([regex]::Escape($ver3))(-beta\.\d+)?$") {
            $issues += "package.json 的 version $($pkg.version) 不符合 baseVersion 基线 $base"
        }

        $tauri = Get-Content (Join-Path $RepoRoot 'src-tauri\tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        $cargoToml = Get-Content (Join-Path $RepoRoot 'src-tauri\Cargo.toml') -Raw -Encoding UTF8
        $cargoTomlVer = ([regex]::Match($cargoToml, '(?m)^version = "([^"]+)"')).Groups[1].Value
        $lock = Get-Content (Join-Path $RepoRoot 'src-tauri\Cargo.lock') -Raw -Encoding UTF8
        $lockVer = ([regex]::Match($lock, '\[\[package\]\]\r?\nname = "git-repo-sync"\r?\nversion = "([^"]+)"')).Groups[1].Value

        if ($tauri.version -ne $pkg.version) {
            $issues += "tauri.conf.json 的 version $($tauri.version) 与 package.json $($pkg.version) 不一致"
        }
        if ($cargoTomlVer -ne $pkg.version) {
            $issues += "Cargo.toml 的 version $cargoTomlVer 与 package.json $($pkg.version) 不一致"
        }
        if ($lockVer -ne $pkg.version) {
            $issues += "Cargo.lock 根包 version $lockVer 与 package.json $($pkg.version) 不一致"
        }
        $wix = $tauri.bundle.windows.wix.version
        if ($wix -ne $pkg.msiVersion) {
            $issues += "wix.version $wix 与 package.json 的 msiVersion $($pkg.msiVersion) 不一致"
        }

        if ($issues.Count -gt 0) {
            Write-Host '项目版本文件与 package.json 不一致，拒绝打 tag：' -ForegroundColor Red
            foreach ($issue in $issues) { Write-Host "  - $issue" }
            throw '请先运行 version.ps1 同步版本文件（或手动修改）后再打 tag。'
        }
        Write-Host "版本一致性检查通过：version=$($pkg.version)，msiVersion=$($pkg.msiVersion)"
    }
    else {
        Write-Host '已用 -Tag 显式指定版本，跳过版本一致性检查。'
    }

    # 2. 确定版本号：优先用 -Tag，否则 v + package.json 的 version（不做推断）
    if (-not $Tag) {
        $Tag = 'v' + $pkg.version
        Write-Host ''
        Write-Host '当前版本配置：'
        Write-Host "  baseVersion : $base（目标版本线）"
        Write-Host "  version     : $($pkg.version)（本次将发布的版本）"
        Write-Host "  msiVersion  : $($pkg.msiVersion)"
        $confirm = Read-Host "请确认已手动修改 version / msiVersion 为本次发布的值？输入 y 继续（将打 tag $Tag），其他任意键取消"
        if ($confirm -ne 'y' -and $confirm -ne 'Y') {
            Write-Host '已取消。请先在 package.json 中手动修改 version / msiVersion，运行 version.ps1 同步后再执行本脚本。'
            return
        }
    }

    if ($Tag -notmatch '^v\d+\.\d+\.\d+(-beta\.\d+)?$') {
        throw "tag 格式不正确：$Tag（应为 vX.Y.Z 或 vX.Y.Z-beta.N，例如 v1.0.0、v1.0.0-beta.1）"
    }
    $Beta = $Tag -match '-beta\.\d+$'

    # 3. 检查本地和远程是否已存在该 tag
    git tag --list $Tag | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'git tag --list 执行失败' }
    if (git tag --list $Tag) {
        throw "本地已存在 tag：$Tag"
    }

    git ls-remote --tags origin $Tag | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'git ls-remote 执行失败' }
    if (git ls-remote --tags origin $Tag) {
        throw "远程已存在 tag：$Tag"
    }
    Write-Host "tag 不存在，可发布：$Tag"

    # 4. 打印将要执行的操作，要求输入 y 确认
    $currentBranch = git rev-parse --abbrev-ref HEAD
    if ($LASTEXITCODE -ne 0) { $currentBranch = '未知' }
    Write-Host ''
    Write-Host '将要执行的操作：'
    Write-Host "  tag  ：$Tag"
    if ($Beta) {
        Write-Host '  类型 ：beta 测试版（GitHub Actions 打包并以 Prerelease 发布）'
    }
    else {
        Write-Host '  类型 ：正式版（触发 GitHub Actions 打包并发布 Release）'
    }
    Write-Host "  分支 ：$currentBranch"
    if ($NoPush) {
        Write-Host '  动作 ：仅创建本地 tag（不推送）'
    }
    else {
        Write-Host '  动作 ：创建 tag 并推送到 origin'
    }
    $answer = Read-Host '确认执行？输入 y 继续，其他任意键取消'
    if ($answer -ne 'y' -and $answer -ne 'Y') {
        Write-Host '已取消，未做任何改动。'
        return
    }

    # 5. 创建带注释的 tag
    git tag -a $Tag -m "Release $Tag"
    if ($LASTEXITCODE -ne 0) { throw "创建 tag $Tag 失败" }
    Write-Host "已创建 tag：$Tag"

    # 6. 推送 tag 触发 Release 工作流
    if ($NoPush) {
        Write-Host "未推送（-NoPush）。如需触发 Release，请执行：git push origin $Tag"
    }
    else {
        git push origin $Tag
        if ($LASTEXITCODE -ne 0) { throw "推送 tag $Tag 失败" }
        Write-Host "已推送 tag $Tag，GitHub Actions 将自动打包并发布 Release。"
        if ($Beta) {
            Write-Host 'beta 版将作为 Prerelease 发布。'
        }
    }
}
finally {
    Pop-Location
}
