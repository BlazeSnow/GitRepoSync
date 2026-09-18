<#
.SYNOPSIS
  快速启动 Git Repo Sync：自动处理终端编码、检查依赖、按需构建并运行 Debug 版。

.DESCRIPTION
  默认启动 Debug 版桌面应用：若未构建过或源码晚于上次构建，则先执行增量构建
  （pnpm tauri build --debug --no-bundle），再后台启动可执行文件——脚本立即返回，
  不阻塞终端，应用独立运行。
  可选参数切换模式：
  -Dev      开发模式：pnpm tauri dev（前端热重载 + Rust 增量编译，占用终端）。
  -Build    打包当前平台发布版安装包（pnpm tauri build）。
  -Rebuild  强制重新构建 Debug 版后再启动。
  首次运行会自动执行 pnpm install 安装前端依赖，可使用 -SkipInstall 跳过。
  脚本开头会将终端切换为 UTF-8 编码（chcp 65001），避免 GBK 终端下中文乱码。

.PARAMETER Dev
  以开发模式启动（pnpm tauri dev，热重载，占用终端直到退出）。

.PARAMETER Build
  打包发布版安装包。

.PARAMETER Rebuild
  强制重新构建 Debug 版后再启动。

.PARAMETER SkipInstall
  跳过依赖安装（pnpm install）。

.EXAMPLE
  .\run.ps1            # 构建并后台启动 Debug 版（源码有改动时自动增量重建）
  .\run.ps1 -Dev       # 开发模式（热重载）
  .\run.ps1 -Build     # 打包发布版
  .\run.ps1 -Rebuild   # 强制重建 Debug 版并启动
#>

[CmdletBinding()]
param(
    [switch]$Dev,
    [switch]$Build,
    [switch]$Rebuild,
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'

# 1. 终端切换为 UTF-8，避免 GBK 终端下中文乱码
chcp 65001 | Out-Null
try { [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new() } catch { }

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $RepoRoot
try {
    # 2. 检查基础依赖
    function Test-Command([string]$Name) {
        return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
    }

    $missing = @()
    if (-not (Test-Command 'node')) { $missing += 'Node.js (>= 20)' }
    if (-not (Test-Command 'pnpm')) { $missing += 'pnpm' }
    if (-not (Test-Command 'cargo')) { $missing += 'Rust (cargo)' }
    if ($missing.Count -gt 0) {
        throw "缺少依赖：$($missing -join '、')。请先安装后再运行本脚本。"
    }

    # 3. 安装前端依赖（首次运行自动安装，可跳过）
    if (-not $SkipInstall -and -not (Test-Path (Join-Path $RepoRoot 'node_modules'))) {
        Write-Host '首次运行，正在安装前端依赖...'
        pnpm install
        if ($LASTEXITCODE -ne 0) { throw 'pnpm install 失败' }
    }

    # 4. 按模式启动
    if ($Dev) {
        pnpm tauri dev
        if ($LASTEXITCODE -ne 0) { throw '运行失败' }
    }
    elseif ($Build) {
        pnpm tauri build
        if ($LASTEXITCODE -ne 0) { throw '打包失败' }
    }
    else {
        # 默认：快速启动 Debug 版；源码晚于上次构建时先增量重建
        $exe = Join-Path $RepoRoot 'src-tauri\target\debug\git-repo-sync.exe'
        $needBuild = $Rebuild -or -not (Test-Path $exe)
        if (-not $needBuild) {
            $exeTime = (Get-Item $exe).LastWriteTimeUtc
            $sources = @(
                (Join-Path $RepoRoot 'src'),
                (Join-Path $RepoRoot 'src-tauri\src'),
                (Join-Path $RepoRoot 'src-tauri\Cargo.toml'),
                (Join-Path $RepoRoot 'src-tauri\tauri.conf.json'),
                (Join-Path $RepoRoot 'index.html'),
                (Join-Path $RepoRoot 'package.json')
            )
            $newest = Get-ChildItem -Recurse -File $sources -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTimeUtc -Descending |
                Select-Object -First 1
            if ($newest -and $newest.LastWriteTimeUtc -gt $exeTime) {
                $needBuild = $true
            }
        }
        if ($needBuild) {
            Write-Host '正在构建 Debug 版（pnpm tauri build --debug --no-bundle）...'
            pnpm tauri build --debug --no-bundle
            if ($LASTEXITCODE -ne 0) { throw 'Debug 构建失败' }
        }
        # 后台启动应用：脚本立即返回，不阻塞终端
        Start-Process -FilePath $exe -WorkingDirectory $RepoRoot | Out-Null
        Write-Host "已在后台启动 $exe"
    }
}
finally {
    Pop-Location
}
