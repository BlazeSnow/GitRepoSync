<#
.SYNOPSIS
  快速启动 Git Repo Sync：自动处理终端编码、检查依赖、按需构建并运行 Debug 版。

.DESCRIPTION
  默认启动 Debug 版桌面应用：若未构建过或源码晚于上次构建，则先执行增量构建
  （pnpm tauri build --debug --no-bundle），再后台启动可执行文件——脚本立即返回，
  不阻塞终端，应用独立运行。
  脚本按阶段输出进度与各阶段耗时（▶ 进行中 / ✓ 完成），构建阶段会注明预计时长：
  Rust 编译与链接是主要耗时来源（改动越多越久，最长可达数分钟）。
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
  .\run.ps1 -Rebuild   # 强制重建 Debug 版并后台启动
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
$TotalStart = Get-Date
Push-Location $RepoRoot
try {
    # 分步进度输出：Start-Step 打印步骤标题，End-Step 打印该步耗时
    $script:StepStart = Get-Date
    function Start-Step([string]$Message) {
        Write-Host ''
        Write-Host "▶ $Message" -ForegroundColor Cyan
        $script:StepStart = Get-Date
    }
    function End-Step {
        $secs = ((Get-Date) - $script:StepStart).TotalSeconds
        Write-Host ("  ✓ 完成（{0:N1} 秒）" -f $secs) -ForegroundColor Green
    }
    function Test-Command([string]$Name) {
        return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
    }

    # 2. 检查基础依赖
    Start-Step '检查依赖（node / pnpm / cargo）...'
    $missing = @()
    if (-not (Test-Command 'node')) { $missing += 'Node.js (>= 20)' }
    if (-not (Test-Command 'pnpm')) { $missing += 'pnpm' }
    if (-not (Test-Command 'cargo')) { $missing += 'Rust (cargo)' }
    if ($missing.Count -gt 0) {
        throw "缺少依赖：$($missing -join '、')。请先安装后再运行本脚本。"
    }
    End-Step

    # 3. 安装前端依赖（首次运行自动安装，可跳过）
    Start-Step '检查前端依赖（node_modules）...'
    if (-not $SkipInstall -and -not (Test-Path (Join-Path $RepoRoot 'node_modules'))) {
        Write-Host '  首次运行，正在安装前端依赖（pnpm install）...'
        pnpm install
        if ($LASTEXITCODE -ne 0) { throw 'pnpm install 失败' }
    }
    else {
        Write-Host '  已安装，跳过安装'
    }
    End-Step

    # 4. 按模式启动
    if ($Dev) {
        Start-Step '检查开发服务器端口（1420）占用...'
        # 清理上次中断残留的 vite 开发服务器（占用 1420 端口的孤儿 node 进程）
        $stale = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty OwningProcess -Unique
        foreach ($ownerPid in $stale) {
            $proc = Get-Process -Id $ownerPid -ErrorAction SilentlyContinue
            if ($proc -and $proc.ProcessName -eq 'node') {
                Write-Host "  清理残留的开发服务器进程（PID $ownerPid）..."
                Stop-Process -Id $ownerPid -Force -ErrorAction SilentlyContinue
            }
            elseif ($proc) {
                throw "端口 1420 被进程 $($proc.ProcessName)（PID $ownerPid）占用，且不是本项目的开发服务器，请手动处理。"
            }
        }
        if (-not $stale) { Write-Host '  端口空闲' }
        End-Step

        # dev 编译会把 exe 覆盖为指向 localhost:1420 的开发变体，删除构建戳，
        # 使下次默认模式的 run.ps1 强制重建为独立运行的变体
        $stampDev = Join-Path $RepoRoot 'src-tauri/target/debug/.grs-build-stamp'
        Remove-Item $stampDev -ErrorAction SilentlyContinue
        Start-Step '启动开发模式（Rust 增量编译 + 前端热重载；改动越多编译越久，窗口出现前请耐心等待；Ctrl+C 退出）...'
        pnpm tauri dev
        if ($LASTEXITCODE -ne 0) { throw '运行失败' }
    }
    elseif ($Build) {
        Start-Step '打包发布版安装包（Rust release 编译 + 链接 + 安装包，通常需要数分钟）...'
        pnpm tauri build
        if ($LASTEXITCODE -ne 0) { throw '打包失败' }
    }
    else {
        # 默认：快速启动 Debug 版；源码晚于上次构建时先增量重建
        $exe = Join-Path $RepoRoot 'src-tauri\target\debug\git-repo-sync.exe'
        Start-Step '检查源码变更（与上次构建产物对比）...'
        # 无构建戳说明 exe 被 -Dev 的编译覆盖过（指向 localhost:1420 的开发变体），必须重建
        $stamp = Join-Path $RepoRoot 'src-tauri/target/debug/.grs-build-stamp'
        $needBuild = $Rebuild -or -not (Test-Path $exe) -or -not (Test-Path $stamp)
        if (-not $needBuild) {
            $exeTime = (Get-Item $exe).LastWriteTimeUtc
            # 注意：单文件不能传给 Get-ChildItem -Recurse（会被当通配模式递归，极慢），
            # 目录用递归枚举，单文件直接 Get-Item
            $newestTime = [DateTime]::MinValue
            $newestFile = $null
            foreach ($dir in @('src', 'src-tauri\src')) {
                Get-ChildItem -Recurse -File (Join-Path $RepoRoot $dir) -ErrorAction SilentlyContinue |
                    ForEach-Object {
                        if ($_.LastWriteTimeUtc -gt $newestTime) {
                            $newestTime = $_.LastWriteTimeUtc
                            $newestFile = $_
                        }
                    }
            }
            foreach ($file in @('src-tauri\Cargo.toml', 'src-tauri\tauri.conf.json', 'index.html', 'package.json')) {
                $fi = Get-Item -LiteralPath (Join-Path $RepoRoot $file) -ErrorAction SilentlyContinue
                if ($fi -and $fi.LastWriteTimeUtc -gt $newestTime) {
                    $newestTime = $fi.LastWriteTimeUtc
                    $newestFile = $fi
                }
            }
            if ($newestFile -and $newestTime -gt $exeTime) {
                $needBuild = $true
                Write-Host ("  检测到变更：{0}" -f $newestFile.FullName.Replace("$RepoRoot\", ''))
            }
        }
        if (-not $needBuild) {
            Write-Host '  源码无变更，跳过构建'
        }
        End-Step

        if ($needBuild) {
            Start-Step '构建 Debug 版（前端构建 + Rust 增量编译与链接——主要耗时来源，改动越多越久，最长可达数分钟）...'
            pnpm tauri build --debug --no-bundle
            if ($LASTEXITCODE -ne 0) { throw 'Debug 构建失败' }
            Set-Content -Path $stamp -Value (Get-Date -Format o)
            End-Step
        }

        Start-Step '后台启动应用...'
        Start-Process -FilePath $exe -WorkingDirectory $RepoRoot | Out-Null
        End-Step
        Write-Host "应用已在后台启动，窗口将很快出现：$exe"
    }

    $total = ((Get-Date) - $TotalStart).TotalSeconds
    Write-Host ''
    Write-Host ('全部完成，总计 {0:N1} 秒' -f $total) -ForegroundColor Cyan
}
finally {
    Pop-Location
}
