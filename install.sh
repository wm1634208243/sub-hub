#!/usr/bin/env bash

# ==============================================================================
# 🚀 SubHub (Clash Sub Hub) 一键管理与极速部署脚本
# 适用系统: Ubuntu / Debian / CentOS / Rocky / AlmaLinux / Alpine / macOS
# ==============================================================================

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

APP_NAME="SubHub"
INSTALL_DIR="/opt/subhub"
REPO_URL="https://github.com/wm1634208243/sub-hub.git"
COMPOSE_FILE="$INSTALL_DIR/docker-compose.yml"
DEFAULT_PORT=3000

# 打印横幅
print_banner() {
    clear
    echo -e "${CYAN}${BOLD}"
    echo "================================================================"
    echo "   🚀 SubHub (Clash Sub Hub) 企业级订阅聚合与分流中台管理脚本   "
    echo "   多订阅聚合 · 实时流量看板 · 通用智能直链 · 节点真机测速   "
    echo "================================================================"
    echo -e "${NC}"
}

# 检查权限
check_root() {
    if [ "$(id -u)" != "0" ]; then
        echo -e "${RED}[错误] 请使用 root 权限或 sudo 运行此脚本！${NC}"
        exit 1
    fi
}

# 检查并安装 Docker & Compose
check_docker() {
    echo -e "${BLUE}[1/3] 检查 Docker 与容器运行环境...${NC}"
    if ! command -v docker &> /dev/null; then
        echo -e "${YELLOW}未检测到 Docker，正在为您自动安装 Docker 环境...${NC}"
        curl -fsSL https://get.docker.com | bash -s docker
        systemctl enable docker
        systemctl start docker
        echo -e "${GREEN}Docker 安装完成！${NC}"
    else
        echo -e "${GREEN}Docker 环境正常。${NC}"
    fi

    if ! docker compose version &> /dev/null && ! command -v docker-compose &> /dev/null; then
        echo -e "${YELLOW}正在安装 Docker Compose 插件...${NC}"
        if [ -x "$(command -v apt-get)" ]; then
            apt-get update && apt-get install -y docker-compose-plugin
        elif [ -x "$(command -v yum)" ]; then
            yum install -y docker-compose-plugin
        fi
    fi
}

# 一键快速安装
install_subhub() {
    print_banner
    check_root
    check_docker

    echo -e "\n${BLUE}[2/3] 配置 SubHub 运行环境与存储目录...${NC}"
    mkdir -p "$INSTALL_DIR/data/configs"

    read -p "请输入 SubHub 外部访问端口 (默认: 3000): " custom_port
    PORT=${custom_port:-$DEFAULT_PORT}

    echo -e "\n${BLUE}[3/3] 正在拉取/构建并启动 SubHub 容器...${NC}"

    cat <<COMPOSE > "$COMPOSE_FILE"
version: '3.8'

services:
  subhub:
    image: node:20-alpine
    container_name: subhub
    restart: always
    working_dir: /app
    ports:
      - "${PORT}:3000"
    volumes:
      - ${INSTALL_DIR}:/app
      - ${INSTALL_DIR}/data:/app/data
    environment:
      - PORT=3000
      - NODE_ENV=production
    command: sh -c "npm install --production && node server.js"
COMPOSE

    # 若目录未包含源码则拉取代码
    if [ ! -f "$INSTALL_DIR/server.js" ]; then
        echo -e "${YELLOW}正在拉取 SubHub 核心程序文件...${NC}"
        if command -v git &> /dev/null; then
            git clone "$REPO_URL" "$INSTALL_DIR/tmp_repo" || true
            if [ -d "$INSTALL_DIR/tmp_repo" ]; then
                cp -rn "$INSTALL_DIR/tmp_repo/"* "$INSTALL_DIR/" || true
                rm -rf "$INSTALL_DIR/tmp_repo"
            fi
        fi
    fi

    cd "$INSTALL_DIR"
    if docker compose version &> /dev/null; then
        docker compose down 2>/dev/null || true
        docker compose up -d --build
    else
        docker-compose down 2>/dev/null || true
        docker-compose up -d --build
    fi

    # 获取本机公网 IP
    IP=$(curl -s https://api.ipify.org || curl -s ifconfig.me || echo "你的服务器IP")

    echo -e "\n${GREEN}================================================================${NC}"
    echo -e "${GREEN}🎉 SubHub 已成功部署并启动！${NC}"
    echo -e "🌐 Web 管理端: ${BOLD}http://${IP}:${PORT}${NC}"
    echo -e "👤 默认初始账号: ${BOLD}admin${NC}"
    echo -e "🔑 默认初始密码: ${BOLD}admin${NC} (请首次登录后立即修改)"
    echo -e "📁 数据存储目录: ${INSTALL_DIR}/data"
    echo -e "${GREEN}================================================================${NC}"
}

# 一键更新
update_subhub() {
    check_root
    if [ ! -d "$INSTALL_DIR" ]; then
        echo -e "${RED}[错误] 未检测到安装目录 $INSTALL_DIR，请先选择安装！${NC}"
        return
    fi
    echo -e "${BLUE}正在更新 SubHub 至最新版本...${NC}"
    cd "$INSTALL_DIR"
    if [ -d ".git" ]; then
        git pull
    fi
    if docker compose version &> /dev/null; then
        docker compose down
        docker compose up -d --build
    else
        docker-compose down
        docker-compose up -d --build
    fi
    echo -e "${GREEN}🎉 SubHub 已成功升级并完成热重启！${NC}"
}

# 服务启停控制
service_control() {
    local action=$1
    check_root
    cd "$INSTALL_DIR" 2>/dev/null || { echo -e "${RED}未安装 SubHub${NC}"; return; }
    if docker compose version &> /dev/null; then
        docker compose "$action"
    else
        docker-compose "$action"
    fi
}

# 查看日志
view_logs() {
    cd "$INSTALL_DIR" 2>/dev/null || { echo -e "${RED}未安装 SubHub${NC}"; return; }
    echo -e "${YELLOW}按 Ctrl+C 可退出日志查看模式${NC}\n"
    if docker compose version &> /dev/null; then
        docker compose logs -f --tail=100
    else
        docker-compose logs -f --tail=100
    fi
}

# 备份数据
backup_data() {
    check_root
    local BACKUP_FILE="/root/subhub_backup_$(date +%Y%m%d_%H%M%S).tar.gz"
    echo -e "${BLUE}正在创建全量数据快照备份...${NC}"
    if [ -d "$INSTALL_DIR/data" ]; then
        tar -czvf "$BACKUP_FILE" -C "$INSTALL_DIR" data
        echo -e "${GREEN}✅ 备份成功！备份文件已保存至: ${BOLD}${BACKUP_FILE}${NC}"
    else
        echo -e "${RED}[错误] 未找到数据目录 $INSTALL_DIR/data${NC}"
    fi
}

# 彻底卸载
uninstall_subhub() {
    check_root
    read -p "⚠️ 确定要彻底卸载 SubHub 吗？(数据将会被移除) [y/N]: " confirm
    if [[ "$confirm" =~ ^[yY]$ ]]; then
        echo -e "${YELLOW}正在停止并移除容器...${NC}"
        cd "$INSTALL_DIR" 2>/dev/null && (docker compose down -v 2>/dev/null || docker-compose down -v 2>/dev/null || true)
        rm -rf "$INSTALL_DIR"
        echo -e "${GREEN}SubHub 已彻底卸载清理完成。${NC}"
    fi
}

# 主菜单交互
show_menu() {
    while true; do
        print_banner
        echo -e " ${BOLD}1.${NC} 🚀 一键安装 / 启动 SubHub (Docker 模式)"
        echo -e " ${BOLD}2.${NC} 🔄 一键无损更新 SubHub 至最新版"
        echo -e " ${BOLD}3.${NC} ⏹️  重启 SubHub 服务"
        echo -e " ${BOLD}4.${NC} ⏸️  停止 SubHub 服务"
        echo -e " ${BOLD}5.${NC} 📋 查看实时运行日志"
        echo -e " ${BOLD}6.${NC} 📦 一键全量数据快照备份 (.tar.gz)"
        echo -e " ${BOLD}7.${NC} 🗑️  彻底卸载 SubHub"
        echo -e " ${BOLD}0.${NC} 退出脚本"
        echo "----------------------------------------------------------------"
        read -p "请输入选项 [0-7]: " choice
        case $choice in
            1) install_subhub; break ;;
            2) update_subhub; break ;;
            3) service_control restart; echo -e "${GREEN}已重启！${NC}"; break ;;
            4) service_control stop; echo -e "${YELLOW}已停止！${NC}"; break ;;
            5) view_logs; break ;;
            6) backup_data; break ;;
            7) uninstall_subhub; break ;;
            0) exit 0 ;;
            *) echo -e "${RED}输入无效，请重新选择${NC}"; sleep 1 ;;
        esac
    done
}

# 直接传参或进入菜单
if [ -n "$1" ]; then
    case "$1" in
        install) install_subhub ;;
        update) update_subhub ;;
        restart) service_control restart ;;
        stop) service_control stop ;;
        logs) view_logs ;;
        backup) backup_data ;;
        uninstall) uninstall_subhub ;;
        *) echo "用法: $0 {install|update|restart|stop|logs|backup|uninstall}" ;;
    esac
else
    show_menu
fi
