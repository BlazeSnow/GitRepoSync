#!/usr/bin/env bash
# CI 辅助脚本：安装 Tauri Linux 构建依赖（Ubuntu）
# - libwebkit2gtk-4.1-dev：Tauri WebView
# - librsvg2-dev：打包图标渲染
# - patchelf：AppImage 打包
# - libayatana-appindicator3-dev：系统托盘
set -euo pipefail

sudo apt-get update
sudo apt-get install -y \
	libwebkit2gtk-4.1-dev \
	librsvg2-dev \
	patchelf \
	libayatana-appindicator3-dev
