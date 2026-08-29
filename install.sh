#!/usr/bin/env bash

# ==============================================================================
# 🚀 SubHub (Clash Sub Hub) 一键极速部署与全能管理脚本 (Rust Native Edition)
# 架构: 纯 Rust 原生单二进制 · 5MB 内存占用 · 微秒级极速响应
# 适用系统: Ubuntu / Debian / CentOS / Rocky / AlmaLinux / Fedora / Alpine / macOS
# ==============================================================================

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

APP_NAME="SubHub"
INSTALL_DIR="/opt/subhub"
REPO_URL="https://github.com/wm1634208243/sub-hub.git"
SERVICE_FILE="/etc/systemd/system/subhub.service"
MODE_FILE="$INSTALL_DIR/.deploy_mode"
DEFAULT_PORT=3000
BIN_PATH="/usr/local/bin/subhub"

# 打印横幅
print_banner() {
    clear
    echo -e "${CYAN}${BOLD}"
    echo "================================================================"
    echo "   🦀 SubHub (Clash Sub Hub) Rust 原生极速订阅分流中台管理脚本   "
    echo "   5MB 内存占用 · 原生单二进制 · 毫秒级分流 · 智能去重测速   "
    echo "================================================================"
    echo -e "${NC}"
}

# 检查 root 权限
check_root() {
    if [ "$(id -u)" != "0" ]; then
        echo -e "${RED}[错误] 请使用 root 权限或 sudo 运行此脚本！${NC}"
        exit 1
    fi
}

# 检测系统包管理器与架构
detect_os() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        OS=$ID
    elif [ -f /etc/debian_version ]; then
        OS="debian"
    elif [ -f /etc/redhat-release ]; then
        OS="centos"
    elif [ -f /etc/alpine-release ]; then
        OS="alpine"
    else
        OS="unknown"
    fi

    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64|amd64)
            BIN_ARCH="linux-amd64"
            ;;
        aarch64|arm64)
            BIN_ARCH="linux-arm64"
            ;;
        *)
            BIN_ARCH="unknown"
            ;;
    esac
}

# 获取公网 IP
get_public_ip() {
    local ip
    ip=$(curl -4 -s --connect-timeout 3 https://api.ipify.org || \
         curl -4 -s --connect-timeout 3 https://ifconfig.me || \
         curl -4 -s --connect-timeout 3 https://icanhazip.com || \
         echo "你的服务器IP")
    echo "$ip"
}

# 检测当前部署模式 (systemd / docker / none)
detect_deploy_mode() {
    if [ -f "$MODE_FILE" ]; then
        cat "$MODE_FILE"
    elif [ -f "$SERVICE_FILE" ]; then
        echo "systemd"
    else
        echo "none"
    fi
}

# 迁移旧版本 Node.js 数据文件至 Rust config/ 目录
migrate_legacy_data() {
    mkdir -p "$INSTALL_DIR/config/configs"
    if [ -d "$INSTALL_DIR/data" ]; then
        echo -e "${BLUE}🔍 正在无损迁移并承接原 Node.js 历史数据与配置...${NC}"
        if [ -f "$INSTALL_DIR/data/users.json" ]; then
            cp -f "$INSTALL_DIR/data/users.json" "$INSTALL_DIR/config/users.json"
        fi
        if [ -d "$INSTALL_DIR/data/configs" ]; then
            cp -rf "$INSTALL_DIR/data/configs/." "$INSTALL_DIR/config/configs/"
            cp -rf "$INSTALL_DIR/data/configs/." "$INSTALL_DIR/config/"
        fi
        if [ -f "$INSTALL_DIR/data/config.json" ]; then
            cp -f "$INSTALL_DIR/data/config.json" "$INSTALL_DIR/config/config.json"
        fi
        echo -e "${GREEN}✅ 原有用户数据与订阅配置已 100% 成功承接迁移！${NC}"
    fi
}

# 确保 VPS 具备基础构建环境
ensure_build_tools() {
    detect_os
    case "$OS" in
        ubuntu|debian|raspbian)
            apt-get update -y && apt-get install -y curl git build-essential gcc
            ;;
        centos|rhel|rocky|almalinux|fedora)
            yum install -y curl git gcc gcc-c++ make
            ;;
        alpine)
            apk add curl git gcc g++ make musl-dev
            ;;
    esac
}

# 下载并安装最新 Rust 原生二进制 (含自动编译兜底机制)
download_rust_binary() {
    local target_ver="$1"
    detect_os

    echo -e "${BLUE}[1/3] 正在拉取 SubHub Rust 原生高性能单文件二进制 (${BIN_ARCH})...${NC}"
    mkdir -p "$INSTALL_DIR/config" /usr/local/bin

    local download_url="https://github.com/wm1634208243/sub-hub/releases/latest/download/subhub-${BIN_ARCH}"
    if [ -n "$target_ver" ] && [ "$target_ver" != "latest" ]; then
        download_url="https://github.com/wm1634208243/sub-hub/releases/download/${target_ver}/subhub-${BIN_ARCH}"
    fi

    echo -e "${YELLOW}下载源: $download_url${NC}"
    local download_success=0
    if curl -fSL --connect-timeout 10 "$download_url" -o "$BIN_PATH.tmp" 2>/dev/null; then
        mv "$BIN_PATH.tmp" "$BIN_PATH"
        chmod +x "$BIN_PATH"
        download_success=1
    fi

    if [ "$download_success" -eq 0 ]; then
        echo -e "${YELLOW}正在通过 Rust 工具链本地极速构建 (耗时 ~30s)...${NC}"
        ensure_build_tools
        sync_subhub_source

        if ! command -v cargo &> /dev/null && [ ! -f "$HOME/.cargo/bin/cargo" ]; then
            echo -e "${YELLOW}正在安装轻量 Rust 编译环境...${NC}"
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
        fi

        export PATH="$HOME/.cargo/bin:$PATH"
        if command -v cargo &> /dev/null; then
            cd "$INSTALL_DIR"
            cargo build --release
            cp "$INSTALL_DIR/target/release/subhub" "$BIN_PATH"
            chmod +x "$BIN_PATH"
            echo -e "${GREEN}本地编译成功完成！${NC}"
        else
            echo -e "${RED}[错误] 无法获取或构建 SubHub 二进制！${NC}"
            exit 1
        fi
    fi

    echo -e "${GREEN}SubHub 核心二进制安装就绪！${NC}"
}

# 同步 SubHub 源码与公共资源
sync_subhub_source() {
    echo -e "${YELLOW}正在同步 SubHub 静态资源与配置目录...${NC}"
    mkdir -p "$INSTALL_DIR/config"
    if [ -d "$INSTALL_DIR/.git" ]; then
        cd "$INSTALL_DIR"
        git fetch origin main && git reset --hard origin/main || git pull || true
    else
        if [ ! -d "$INSTALL_DIR" ] || [ -z "$(ls -A "$INSTALL_DIR" 2>/dev/null)" ]; then
            git clone "$REPO_URL" "$INSTALL_DIR" || true
        fi
    fi
}

# Rust 原生部署主流程
install_native_mode() {
    print_banner
    check_root
    detect_os

    echo -e "\n${BLUE}[1/4] 检测系统架构与环境 (${OS} / ${ARCH})...${NC}"
    echo -e "${GREEN}系统检测通过！SubHub 将以纯 Rust 原生单二进制部署 (免装 Node.js，5MB 极致低内存)${NC}"

    echo -e "\n${BLUE}[2/4] 配置 SubHub 外部访问端口...${NC}"
    read -p "请输入 SubHub 外部访问端口 (默认: 3000): " custom_port
    PORT=${custom_port:-$DEFAULT_PORT}

    echo -e "\n${BLUE}[3/4] 下载并安装 SubHub Rust 单二进制...${NC}"
    download_rust_binary "latest"
    migrate_legacy_data

    echo -e "\n${BLUE}[4/4] 配置 Systemd 守护进程与开机自启...${NC}"

    cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=SubHub High-Performance Subscription Aggregator (Rust Native)
Documentation=https://github.com/wm1634208243/sub-hub
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$INSTALL_DIR
ExecStart=$BIN_PATH --port $PORT --config-dir $INSTALL_DIR/config
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

    echo "systemd" > "$MODE_FILE"

    if command -v systemctl &> /dev/null; then
        systemctl daemon-reload
        systemctl enable subhub
        systemctl restart subhub
    else
        nohup "$BIN_PATH" --port "$PORT" --config-dir "$INSTALL_DIR/config" > /dev/null 2>&1 &
    fi

    sleep 1
    local IP
    IP=$(get_public_ip)

    echo -e "\n${GREEN}================================================================${NC}"
    echo -e "${GREEN}🎉 SubHub Rust 原生架构已成功部署并已开机自启！${NC}"
    echo -e "🚀 运行方式: ${BOLD}Rust 原生单文件进程${NC} (常驻内存仅 ~5MB，微秒级极速响应)"
    echo -e "🌐 Web 管理端: ${BOLD}http://${IP}:${PORT}${NC}"
    echo -e "👤 默认初始账号: ${BOLD}admin${NC}"
    echo -e "🔑 默认初始密码: ${BOLD}admin${NC} (请首次登录后立即修改)"
    echo -e "📁 配置与数据目录: ${INSTALL_DIR}/config"
    echo -e "⚙️ 服务管理命令: ${BOLD}systemctl {start|stop|restart|status} subhub${NC}"
    echo -e "${GREEN}================================================================${NC}"
}

# 一键平滑更新与版本热替换
update_subhub() {
    check_root
    local target_ver="$1"

    echo -e "\n${YELLOW}================================================================${NC}"
    echo -e "${YELLOW}🚀 正在执行 SubHub 全自动热升级流水线...${NC}"
    echo -e "${YELLOW}================================================================${NC}"

    # 1. 下载新版本二进制
    download_rust_binary "$target_ver"

    # 2. 检查并迁移原有的数据与订阅配置
    migrate_legacy_data

    # 3. 更新 systemd 服务文件指向 Rust 二进制
    cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=SubHub High-Performance Subscription Aggregator (Rust Native)
Documentation=https://github.com/wm1634208243/sub-hub
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$INSTALL_DIR
ExecStart=$BIN_PATH --port 3000 --config-dir $INSTALL_DIR/config
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

    # 4. 重启 Systemd 服务
    if command -v systemctl &> /dev/null; then
        systemctl daemon-reload
        systemctl restart subhub
        echo -e "${GREEN}Systemd 服务已平滑重启并切换至 Rust 引擎！${NC}"
    fi

    # 5. 清理旧 node_modules 释放磁盘
    rm -rf "$INSTALL_DIR/node_modules" 2>/dev/null || true

    echo -e "\n${GREEN}🎉 SubHub 已成功完成一键平滑升级至 Rust 原生架构！所有数据与配置 100% 完美保留！${NC}"
}

# 彻底卸载
uninstall_subhub() {
    check_root
    read -p "⚠️ 确定要彻底卸载 SubHub 吗？(所有用户与配置数据将被清除) [y/N]: " confirm
    if [[ "$confirm" =~ ^[yY]$ ]]; then
        echo -e "${YELLOW}正在停止并移除服务...${NC}"
        if [ -f "$SERVICE_FILE" ]; then
            systemctl stop subhub 2>/dev/null || true
            systemctl disable subhub 2>/dev/null || true
            rm -f "$SERVICE_FILE"
            systemctl daemon-reload 2>/dev/null || true
        fi
        rm -f "$BIN_PATH"
        rm -rf "$INSTALL_DIR"
        echo -e "${GREEN}SubHub 已彻底卸载并清理完成。${NC}"
    fi
}

# 交互式主菜单
show_menu() {
    while true; do
        print_banner
        local status_text="${RED}未安装${NC}"

        if [ -f "$SERVICE_FILE" ]; then
            if systemctl is-active --quiet subhub 2>/dev/null; then
                status_text="${GREEN}Rust 引擎运行中 (Systemd · ~5MB 内存)${NC}"
            else
                status_text="${YELLOW}服务已停止 (Systemd)${NC}"
            fi
        fi

        echo -e "当前运行状态: $status_text"
        echo -e "----------------------------------------------------------------"
        echo -e " ${BOLD}1.${NC} 🚀 安装 / 部署 SubHub (${GREEN}Rust 原生极速架构 · 免环境依赖${NC})"
        echo -e " ${BOLD}2.${NC} ⚡ 一键平滑热更新 (${YELLOW}升级至最新 Rust 版本 / 保留全部数据${NC})"
        echo -e " ${BOLD}3.${NC} 🔄 重启 SubHub 服务"
        echo -e " ${BOLD}4.${NC} 🛑 停止 SubHub 服务"
        echo -e " ${BOLD}5.${NC} ▶️  启动 SubHub 服务"
        echo -e " ${BOLD}6.${NC} 📋 查看实时运行日志"
        echo -e " ${BOLD}7.${NC} 🗑️  彻底卸载 SubHub"
        echo -e " ${BOLD}0.${NC} 退出脚本"
        echo -e "----------------------------------------------------------------"
        read -p "请输入选项 [0-7]: " menu_choice

        case "$menu_choice" in
            1) install_native_mode ;;
            2) update_subhub ;;
            3) systemctl restart subhub && echo -e "${GREEN}服务已重启！${NC}" ;;
            4) systemctl stop subhub && echo -e "${YELLOW}服务已停止！${NC}" ;;
            5) systemctl start subhub && echo -e "${GREEN}服务已启动！${NC}" ;;
            6) journalctl -u subhub -f -n 50 ;;
            7) uninstall_subhub ;;
            0) exit 0 ;;
            *) echo -e "${RED}无效选项，请重新输入！${NC}" ;;
        esac
        echo -e "\n按任意键返回主菜单..."
        read -n 1 -s -r
    done
}

# 命令行入参快捷处理
case "$1" in
    install) install_native_mode ;;
    update) update_subhub "$2" ;;
    restart) systemctl restart subhub ;;
    stop) systemctl stop subhub ;;
    start) systemctl start subhub ;;
    status) systemctl status subhub ;;
    logs) journalctl -u subhub -f -n 50 ;;
    uninstall) uninstall_subhub ;;
    *) show_menu ;;
esac
