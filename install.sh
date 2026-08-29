#!/usr/bin/env bash

# ==============================================================================
# 🚀 SubHub (Clash Sub Hub) 一键极速部署与全能管理脚本
# 支持模式: 1. 原生 Node.js + Systemd (极低内存) | 2. Docker 容器化
# 适用系统: Ubuntu / Debian / CentOS / Rocky / AlmaLinux / Fedora / Alpine
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
COMPOSE_FILE="$INSTALL_DIR/docker-compose.yml"
SERVICE_FILE="/etc/systemd/system/subhub.service"
MODE_FILE="$INSTALL_DIR/.deploy_mode"
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

# 检查 root 权限
check_root() {
    if [ "$(id -u)" != "0" ]; then
        echo -e "${RED}[错误] 请使用 root 权限或 sudo 运行此脚本！${NC}"
        exit 1
    fi
}

# 检测系统包管理器
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
    elif [ -f "$COMPOSE_FILE" ]; then
        echo "docker"
    else
        echo "none"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# 1. 原生 Node.js + Systemd 部署
# ─────────────────────────────────────────────────────────────────────────────

# 检查并安装 Node.js 18+ 环境
check_and_install_nodejs() {
    echo -e "${BLUE}[1/4] 检查 Node.js 运行环境...${NC}"
    detect_os

    local need_install=0
    if command -v node &> /dev/null; then
        local node_ver
        node_ver=$(node -v | sed 's/v//' | cut -d. -f1)
        if [ "$node_ver" -ge 18 ]; then
            echo -e "${GREEN}检测到 Node.js 已安装 (版本: $(node -v))，符合要求。${NC}"
            return 0
        else
            echo -e "${YELLOW}检测到已安装的 Node.js 版本 ($(node -v)) 低于 v18，正在升级...${NC}"
            need_install=1
        fi
    else
        echo -e "${YELLOW}未检测到 Node.js，正在为您自动安装 Node.js LTS (v20.x)...${NC}"
        need_install=1
    fi

    if [ "$need_install" -eq 1 ]; then
        case "$OS" in
            ubuntu|debian|raspbian)
                apt-get update -y
                apt-get install -y curl gnupg git build-essential
                curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
                apt-get install -y nodejs
                ;;
            centos|rhel|rocky|almalinux|fedora)
                if command -v dnf &> /dev/null; then
                    dnf install -y curl git make gcc-c++
                    curl -fsSL https://rpm.nodesource.com/setup_20.x | bash -
                    dnf install -y nodejs
                else
                    yum install -y curl git make gcc-c++
                    curl -fsSL https://rpm.nodesource.com/setup_20.x | bash -
                    yum install -y nodejs
                fi
                ;;
            alpine)
                apk update
                apk add nodejs npm git curl make gcc g++
                ;;
            *)
                echo -e "${RED}[错误] 未识别的 Linux 发行版，尝试使用包管理器安装...${NC}"
                if command -v apt-get &> /dev/null; then
                    apt-get update && apt-get install -y nodejs npm git
                elif command -v yum &> /dev/null; then
                    yum install -y nodejs npm git
                fi
                ;;
        esac
    fi

    if ! command -v node &> /dev/null; then
        echo -e "${RED}[错误] Node.js 自动安装失败，请手动安装 Node.js 18+ 后重试！${NC}"
        exit 1
    fi
    echo -e "${GREEN}Node.js 环境就绪: $(node -v), npm $(npm -v)${NC}"
}

# 同步 SubHub 源码 (安全兼容已存在目录与增量覆盖)
sync_subhub_source() {
    echo -e "${YELLOW}正在同步 SubHub 核心程序文件...${NC}"
    if [ -d "$INSTALL_DIR/.git" ]; then
        echo -e "${GREEN}检测到已有 Git 仓库，正在拉取最新代码...${NC}"
        cd "$INSTALL_DIR"
        git fetch origin main && git reset --hard origin/main || git pull || true
    else
        if [ ! -d "$INSTALL_DIR" ] || [ -z "$(ls -A "$INSTALL_DIR" 2>/dev/null)" ]; then
            git clone "$REPO_URL" "$INSTALL_DIR"
        else
            echo -e "${YELLOW}检测到目录已存在，正在克隆并覆盖更新源码...${NC}"
            local tmp_clone="/tmp/subhub_git_tmp_$$"
            rm -rf "$tmp_clone"
            git clone "$REPO_URL" "$tmp_clone"
            mkdir -p "$INSTALL_DIR"
            cp -r "$tmp_clone/." "$INSTALL_DIR/"
            rm -rf "$tmp_clone"
        fi
    fi
    mkdir -p "$INSTALL_DIR/data/configs"
}

# 原生安装主流程
install_native_mode() {
    print_banner
    check_root
    check_and_install_nodejs

    echo -e "\n${BLUE}[2/4] 配置 SubHub 外部访问端口...${NC}"
    read -p "请输入 SubHub 外部访问端口 (默认: 3000): " custom_port
    PORT=${custom_port:-$DEFAULT_PORT}

    echo -e "\n${BLUE}[3/4] 同步 SubHub 核心代码与依赖...${NC}"
    sync_subhub_source

    cd "$INSTALL_DIR"
    echo -e "${YELLOW}正在安装生产环境运行依赖 (npm install)...${NC}"
    npm install --production

    echo -e "\n${BLUE}[4/4] 配置 Systemd 守护进程与开机自启...${NC}"
    local NODE_BIN
    NODE_BIN=$(command -v node)

    cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=SubHub Subscription Aggregator Service
Documentation=https://github.com/wm1634208243/sub-hub
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$INSTALL_DIR
ExecStart=$NODE_BIN server.js
Restart=always
RestartSec=5
Environment=PORT=$PORT
Environment=NODE_ENV=production
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
        # 兼容非 systemd 环境 (如某些精简容器或环境)
        nohup "$NODE_BIN" server.js > /dev/null 2>&1 &
    fi

    sleep 1
    local IP
    IP=$(get_public_ip)

    echo -e "\n${GREEN}================================================================${NC}"
    echo -e "${GREEN}🎉 SubHub 原生模式已成功部署并已开机自启！${NC}"
    echo -e "🚀 运行方式: ${BOLD}原生 Node.js + Systemd 守护进程${NC} (极低内存占用)"
    echo -e "🌐 Web 管理端: ${BOLD}http://${IP}:${PORT}${NC}"
    echo -e "👤 默认初始账号: ${BOLD}admin${NC}"
    echo -e "🔑 默认初始密码: ${BOLD}admin${NC} (请首次登录后立即修改)"
    echo -e "📁 程序根目录: ${INSTALL_DIR}"
    echo -e "📁 数据存储目录: ${INSTALL_DIR}/data"
    echo -e "⚙️ 服务管理命令: ${BOLD}systemctl {start|stop|restart|status} subhub${NC}"
    echo -e "${GREEN}================================================================${NC}"
}

# ─────────────────────────────────────────────────────────────────────────────
# 2. Docker 容器化部署
# ─────────────────────────────────────────────────────────────────────────────

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

install_docker_mode() {
    print_banner
    check_root
    check_docker

    echo -e "\n${BLUE}[2/3] 配置 SubHub 容器端口与存储目录...${NC}"
    read -p "请输入 SubHub 外部映射端口 (默认: 3000): " custom_port
    PORT=${custom_port:-$DEFAULT_PORT}

    echo -e "\n${BLUE}[3/3] 同步源码并启动 SubHub 容器...${NC}"
    sync_subhub_source

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

    echo "docker" > "$MODE_FILE"

    cd "$INSTALL_DIR"
    if docker compose version &> /dev/null; then
        docker compose down 2>/dev/null || true
        docker compose up -d --build
    else
        docker-compose down 2>/dev/null || true
        docker-compose up -d --build
    fi

    local IP
    IP=$(get_public_ip)

    echo -e "\n${GREEN}================================================================${NC}"
    echo -e "${GREEN}🎉 SubHub Docker 容器化模式已成功部署并启动！${NC}"
    echo -e "🚀 运行方式: ${BOLD}Docker 容器隔离模式${NC}"
    echo -e "🌐 Web 管理端: ${BOLD}http://${IP}:${PORT}${NC}"
    echo -e "👤 默认初始账号: ${BOLD}admin${NC}"
    echo -e "🔑 默认初始密码: ${BOLD}admin${NC} (请首次登录后立即修改)"
    echo -e "📁 宿主机数据目录: ${INSTALL_DIR}/data"
    echo -e "${GREEN}================================================================${NC}"
}

# ─────────────────────────────────────────────────────────────────────────────
# 3. 日常运维管理 (自动适配 Systemd 与 Docker)
# ─────────────────────────────────────────────────────────────────────────────

# 一键更新与版本切换 (支持升级最新版或切换/回退至指定历史版本)
update_subhub() {
    check_root
    local target_ver="$1"

    if [ ! -d "$INSTALL_DIR" ]; then
        echo -e "${RED}[错误] 未检测到安装目录 $INSTALL_DIR，请先执行安装！${NC}"
        return
    fi

    if [ -z "$target_ver" ]; then
        echo -e "\n请选择更新与版本切换模式:"
        echo -e " ${BOLD}1.${NC} 🚀 升级至 GitHub 官方最新稳定版 (${GREEN}推荐${NC})"
        echo -e " ${BOLD}2.${NC} 🎯 切换 / 回退至指定历史版本 (${YELLOW}如 v1.1.6, v1.1.5 等${NC})"
        read -p "请选择 [1-2, 默认: 1]: " up_choice
        up_choice=${up_choice:-1}
        if [ "$up_choice" = "2" ]; then
            read -p "请输入目标版本号 (例如 v1.0.3): " input_ver
            target_ver="$input_ver"
        fi
    fi

    local mode
    mode=$(detect_deploy_mode)
    local is_specific=false
    local target_tag=""
    if [ -n "$target_ver" ] && [ "$target_ver" != "latest" ]; then
        is_specific=true
        target_tag=$(echo "$target_ver" | sed -e 's/^v//')
        target_tag="v$target_tag"
        echo -e "${BLUE}正在将 SubHub 切换至版本 【${BOLD}${target_tag}${NC}】 (运行模式: ${BOLD}${mode}${NC})...${NC}"
    else
        echo -e "${BLUE}正在更新 SubHub 至官方最新版本 (运行模式: ${BOLD}${mode}${NC})...${NC}"
    fi

    cd "$INSTALL_DIR"
    if [ -d ".git" ]; then
        if [ "$is_specific" = true ]; then
            git fetch origin main --tags
            git checkout "tags/$target_tag" 2>/dev/null || git checkout "$target_tag" 2>/dev/null || git checkout "$target_ver" 2>/dev/null || {
                echo -e "${YELLOW}未找到标签 $target_tag，正在拉取最新代码...${NC}"
                git reset --hard origin/main
            }
        else
            git fetch origin main && git reset --hard origin/main
        fi
    else
        echo -e "${YELLOW}未检测到 git 仓库，正在重新初始化拉取...${NC}"
        git clone "$REPO_URL" tmp_git
        cp -rn tmp_git/* "$INSTALL_DIR/" || true
        rm -rf tmp_git
    fi

    if [ "$mode" = "systemd" ]; then
        npm install --production
        systemctl restart subhub
        echo -e "${GREEN}🎉 SubHub (原生模式) 已成功切换并热重启！${NC}"
    elif [ "$mode" = "docker" ]; then
        if docker compose version &> /dev/null; then
            docker compose down
            docker compose up -d --build
        else
            docker-compose down
            docker-compose up -d --build
        fi
        echo -e "${GREEN}🎉 SubHub (Docker 模式) 已完成镜像构建并热重启！${NC}"
    else
        if systemctl is-active --quiet subhub 2>/dev/null; then
            npm install --production
            systemctl restart subhub
            echo -e "${GREEN}🎉 SubHub 已成功切换并重启！${NC}"
        else
            echo -e "${YELLOW}请根据你的运行方式手动重启 SubHub。${NC}"
        fi
    fi
}

# 服务启停控制
service_control() {
    local action=$1
    check_root
    local mode
    mode=$(detect_deploy_mode)

    case "$action" in
        start)
            if [ "$mode" = "systemd" ]; then
                systemctl start subhub
            elif [ "$mode" = "docker" ]; then
                cd "$INSTALL_DIR" && (docker compose start 2>/dev/null || docker-compose start)
            else
                systemctl start subhub 2>/dev/null || true
            fi
            echo -e "${GREEN}SubHub 服务已启动！${NC}"
            ;;
        stop)
            if [ "$mode" = "systemd" ]; then
                systemctl stop subhub
            elif [ "$mode" = "docker" ]; then
                cd "$INSTALL_DIR" && (docker compose stop 2>/dev/null || docker-compose stop)
            else
                systemctl stop subhub 2>/dev/null || true
            fi
            echo -e "${YELLOW}SubHub 服务已停止！${NC}"
            ;;
        restart)
            if [ "$mode" = "systemd" ]; then
                systemctl restart subhub
            elif [ "$mode" = "docker" ]; then
                cd "$INSTALL_DIR" && (docker compose restart 2>/dev/null || docker-compose restart)
            else
                systemctl restart subhub 2>/dev/null || true
            fi
            echo -e "${GREEN}SubHub 服务已重启完成！${NC}"
            ;;
    esac
}

# 查看实时日志
view_logs() {
    local mode
    mode=$(detect_deploy_mode)
    echo -e "${YELLOW}按 Ctrl+C 可退出实时日志查看模式${NC}\n"

    if [ "$mode" = "systemd" ]; then
        journalctl -u subhub -f -n 100
    elif [ "$mode" = "docker" ]; then
        cd "$INSTALL_DIR" 2>/dev/null || true
        if docker compose version &> /dev/null; then
            docker compose logs -f --tail=100
        else
            docker-compose logs -f --tail=100
        fi
    else
        journalctl -u subhub -f -n 100 2>/dev/null || (cd "$INSTALL_DIR" && docker compose logs -f --tail=100 2>/dev/null)
    fi
}

# 全量数据备份
backup_data() {
    check_root
    local BACKUP_FILE="/root/subhub_backup_$(date +%Y%m%d_%H%M%S).tar.gz"
    echo -e "${BLUE}正在创建 SubHub 全量数据快照备份 (.tar.gz)...${NC}"
    if [ -d "$INSTALL_DIR/data" ]; then
        tar -czvf "$BACKUP_FILE" -C "$INSTALL_DIR" data
        echo -e "${GREEN}✅ 备份成功！文件已存至: ${BOLD}${BACKUP_FILE}${NC}"
    else
        echo -e "${RED}[错误] 未找到数据目录 $INSTALL_DIR/data${NC}"
    fi
}

# 域名反向代理与 HTTPS 证书配置助手
setup_domain_ssl() {
    print_banner
    check_root
    echo -e "${CYAN}${BOLD}=== 🌐 SubHub 域名绑定与 HTTPS 反向代理配置助手 ===${NC}\n"
    read -p "请输入您已解析到本机的域名 (如 sub.example.com): " custom_domain
    if [ -z "$custom_domain" ]; then
        echo -e "${RED}[错误] 域名不能为空！${NC}"
        return
    fi
    custom_domain=$(echo "$custom_domain" | sed -e 's|^https\?://||' -e 's|/.*$||')

    # 自动探测 SubHub 运行端口 (原生环境或 Docker 映射端口)
    local target_port="3000"
    if [ -f "$INSTALL_DIR/.env" ]; then
        local env_port=$(grep -E '^PORT=' "$INSTALL_DIR/.env" | cut -d'=' -f2 | tr -d '"' | tr -d "'" | tr -d '\r')
        [ -n "$env_port" ] && target_port="$env_port"
    elif docker ps --format '{{.Ports}}' 2>/dev/null | grep -q "clash-sub-hub"; then
        local docker_port=$(docker ps --format '{{.Ports}}' | grep "clash-sub-hub" | sed -E 's/.*:([0-9]+)->3000.*/\1/')
        [ -n "$docker_port" ] && target_port="$docker_port"
    fi
    read -p "请输入 SubHub 当前后端端口 [默认: $target_port]: " input_port
    target_port="${input_port:-$target_port}"

    echo -e "\n请选择反向代理与访问绑定模式:"
    echo -e " ${BOLD}1.${NC} ⚡ Caddy (${GREEN}推荐 · 支持 443 / 8443 / 2096 任意端口 · 全自动申请 SSL 证书${NC})"
    echo -e " ${BOLD}2.${NC} 🚀 原生端口直连模式 (${CYAN}免反代 · 直接使用 http://域名:$target_port 访问与下发直链${NC})"
    echo -e " ${BOLD}3.${NC} ☁️  Cloudflare CDN / Tunnel 模式 (${YELLOW}开启小黄云或 Tunnel 隧道${NC})"
    echo -e " ${BOLD}4.${NC} 🛡️  Nginx + Certbot (${MAGENTA}标准 Nginx 反代模板${NC})"
    read -p "请选择 [1-4, 默认: 1]: " ssl_choice
    ssl_choice=${ssl_choice:-1}

    local final_custom_domain="https://${custom_domain}"

    case "$ssl_choice" in
        1)
            read -p "请输入 Caddy 对外访问端口 [默认: 443 (无端口), 或 8443 / 2096 / 3000]: " ext_port
            ext_port=${ext_port:-443}

            local caddy_site="$custom_domain"
            if [ "$ext_port" != "443" ] && [ "$ext_port" != "80" ]; then
                caddy_site="${custom_domain}:${ext_port}"
                final_custom_domain="https://${custom_domain}:${ext_port}"
            else
                final_custom_domain="https://${custom_domain}"
            fi

            echo -e "\n${YELLOW}正在安装并配置 Caddy 自动化反代引擎 (监听: $caddy_site -> 127.0.0.1:$target_port)...${NC}"
            detect_os
            if ! command -v caddy &> /dev/null; then
                case "$OS" in
                    ubuntu|debian|raspbian)
                        apt-get update && apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
                        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
                        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
                        apt-get update && apt-get install -y caddy
                        ;;
                    centos|rhel|rocky|almalinux|fedora)
                        yum install -y yum-plugin-copr
                        yum copr enable -y @caddy/caddy
                        yum install -y caddy
                        ;;
                    alpine)
                        apk add caddy
                        ;;
                esac
            fi

            mkdir -p /etc/caddy
            cat <<CADDY > /etc/caddy/Caddyfile
$caddy_site {
    reverse_proxy 127.0.0.1:$target_port
}
CADDY
            systemctl enable caddy 2>/dev/null || true
            systemctl restart caddy 2>/dev/null || true
            echo -e "\n${GREEN}🎉 Caddy 已配置完成！已自动监听 $ext_port 端口并申请 SSL 证书！${NC}"
            echo -e "🌐 您现在可直接访问: ${BOLD}${final_custom_domain}${NC}"
            ;;
        2)
            read -p "请输入直连访问端口 [默认: $target_port]: " ext_port
            ext_port=${ext_port:-$target_port}
            final_custom_domain="http://${custom_domain}:${ext_port}"
            echo -e "\n${GREEN}🎉 已切换为原生端口直连模式！${NC}"
            echo -e "🌐 全局直链地址已绑定为: ${BOLD}${final_custom_domain}${NC}"
            ;;
        3)
            echo -e "\n${CYAN}☁️ Cloudflare CDN / Tunnel 模式配置说明:${NC}"
            echo -e "1. 在 Cloudflare DNS 面板添加 A 记录: ${BOLD}${custom_domain}${NC} -> 本机公网 IP"
            echo -e "2. 开启 ${YELLOW}小黄云代理 (Proxied)${NC}并在 Origin Rules 将 443 重写至 $target_port"
            echo -e "3. 或在 Cloudflare Zero Trust 中创建 Cloudflare Tunnel 映射至 localhost:$target_port"
            echo -e "4. 全局直链将自动生效为: ${BOLD}https://${custom_domain}${NC}"
            final_custom_domain="https://${custom_domain}"
            ;;
        4)
            read -p "请输入 Nginx 对外监听端口 [默认: 80 / 443]: " ext_port
            ext_port=${ext_port:-80}
            echo -e "\n${GREEN}Nginx 配置文件参考 (保存在 /etc/nginx/conf.d/subhub.conf):${NC}"
            cat <<NGINX
server {
    listen $ext_port;
    server_name $custom_domain;
    location / {
        proxy_pass http://127.0.0.1:$target_port;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
NGINX
            final_custom_domain="http://${custom_domain}:${ext_port}"
            echo -e "\n使用 ${BOLD}certbot --nginx -d $custom_domain${NC} 即可自动签发 SSL！"
            ;;
    esac

    # 同步写入 SubHub 系统配置
    if [ -d "$INSTALL_DIR/data" ]; then
        node -e "
        const fs = require('fs');
        const file = '$INSTALL_DIR/data/system_settings.json';
        let s = {};
        try { s = JSON.parse(fs.readFileSync(file, 'utf8')); } catch {}
        s.customDomain = '$final_custom_domain';
        s.updatedAt = new Date().toISOString();
        fs.writeFileSync(file, JSON.stringify(s, null, 2));
        " 2>/dev/null || true
    fi
}

# 彻底卸载
uninstall_subhub() {
    check_root
    local mode
    mode=$(detect_deploy_mode)
    read -p "⚠️ 确定要彻底卸载 SubHub 吗？(所有用户与配置数据将被清除) [y/N]: " confirm
    if [[ "$confirm" =~ ^[yY]$ ]]; then
        echo -e "${YELLOW}正在停止并移除服务与容器...${NC}"
        if [ "$mode" = "systemd" ] || [ -f "$SERVICE_FILE" ]; then
            systemctl stop subhub 2>/dev/null || true
            systemctl disable subhub 2>/dev/null || true
            rm -f "$SERVICE_FILE"
            systemctl daemon-reload 2>/dev/null || true
        fi
        if [ "$mode" = "docker" ] || [ -f "$COMPOSE_FILE" ]; then
            cd "$INSTALL_DIR" 2>/dev/null && (docker compose down -v 2>/dev/null || docker-compose down -v 2>/dev/null || true)
        fi
        rm -rf "$INSTALL_DIR"
        echo -e "${GREEN}SubHub 已彻底卸载并清理完成。${NC}"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# 4. 交互式主菜单
# ─────────────────────────────────────────────────────────────────────────────

show_menu() {
    while true; do
        print_banner
        local mode
        mode=$(detect_deploy_mode)
        local status_text="${RED}未安装${NC}"

        if [ "$mode" = "systemd" ]; then
            if systemctl is-active --quiet subhub 2>/dev/null; then
                status_text="${GREEN}原生运行中 (Systemd)${NC}"
            else
                status_text="${YELLOW}原生已停止 (Systemd)${NC}"
            fi
        elif [ "$mode" = "docker" ]; then
            if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "subhub"; then
                status_text="${GREEN}容器运行中 (Docker)${NC}"
            else
                status_text="${YELLOW}容器已停止 (Docker)${NC}"
            fi
        fi

        echo -e " 当前运行状态: ${status_text}"
        echo "----------------------------------------------------------------"
        echo -e " ${BOLD}[安装与部署模式]${NC}"
        echo -e " ${BOLD}1.${NC} 🚀 原生 Node.js + Systemd 极速部署 (${GREEN}推荐 · 极低内存 · 开机自启${NC})"
        echo -e " ${BOLD}2.${NC} 🐳 Docker 容器化一键部署 (${CYAN}隔离免配环境${NC})"
        echo ""
        echo -e " ${BOLD}[日常运维与高级配置]${NC}"
        echo -e " ${BOLD}3.${NC} 🔄 一键更新 / 切换至指定版本 (${YELLOW}支持历史版本回退${NC})"
        echo -e " ${BOLD}4.${NC} ▶️  启动 SubHub 服务"
        echo -e " ${BOLD}5.${NC} ⏹️  重启 SubHub 服务"
        echo -e " ${BOLD}6.${NC} ⏸️  停止 SubHub 服务"
        echo -e " ${BOLD}7.${NC} 📋 查看实时运行日志"
        echo -e " ${BOLD}8.${NC} 🌐 域名绑定与 HTTPS 证书配置 (${CYAN}Caddy / Cloudflare / Nginx${NC})"
        echo -e " ${BOLD}9.${NC} 📦 一键全量数据快照备份 (.tar.gz)"
        echo -e " ${BOLD}10.${NC} 🗑️  彻底卸载 SubHub"
        echo -e " ${BOLD}0.${NC} 退出脚本"
        echo "----------------------------------------------------------------"
        read -p "请输入选项 [0-10]: " choice
        case $choice in
            1) install_native_mode; break ;;
            2) install_docker_mode; break ;;
            3) update_subhub; break ;;
            4) service_control start; break ;;
            5) service_control restart; break ;;
            6) service_control stop; break ;;
            7) view_logs; break ;;
            8) setup_domain_ssl; break ;;
            9) backup_data; break ;;
            10) uninstall_subhub; break ;;
            0) exit 0 ;;
            *) echo -e "${RED}输入无效，请重新选择${NC}"; sleep 1 ;;
        esac
    done
}

# 直接传参或进入交互式菜单
if [ -n "$1" ]; then
    case "$1" in
        install|native) install_native_mode ;;
        docker) install_docker_mode ;;
        update) update_subhub "$2" ;;
        start) service_control start ;;
        restart) service_control restart ;;
        stop) service_control stop ;;
        logs) view_logs ;;
        domain|ssl) setup_domain_ssl ;;
        backup) backup_data ;;
        uninstall) uninstall_subhub ;;
        *) echo "用法: $0 {install|docker|update|start|restart|stop|logs|domain|backup|uninstall}" ;;
    esac
else
    show_menu
fi
