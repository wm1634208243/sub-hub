<div align="center">

# 🚀 Clash Sub Hub (SubHub)

### 新一代企业级通用订阅聚合、智能分流覆写与流量看板中台
**Modern, High-Performance Universal Subscription Aggregator, Live Quota Dashboard & Rule Override Hub**

[![Node.js Version](https://img.shields.io/badge/Node.js-18%2B-green.svg)](https://nodejs.org/)
[![Docker Pulls](https://img.shields.io/badge/Docker-Ready-blue.svg)](https://www.docker.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/wm1634208243/sub-hub)

[✨ 功能特性](#-核心功能特性) • [📊 对比 Sub-Store](#-subhub-vs-sub-store-深度功能对比) • [🛠️ 快速部署](#️-快速部署指南) • [📱 客户端配置](#-全平台客户端接入指南) • [🔌 API 接口](#-开放订阅端点一览)

</div>

---

## 💡 为什么选择 SubHub？

在日常科学上网和网络代理管理中，我们常常面临以下痛点：
- **订阅源碎片化**：手头有自建 VPS（3X-UI/X-UI）、多个商业机场订阅，每个客户端都要重复导入、维护多个订阅链接；
- **流量/到期难以统揽**：无法直观知道哪个订阅快用完了、哪个 VPS 节点下个月要续费；
- **节点命名杂乱/死节点多**：上游机场常带有冗长的广告、倍率说明（如 `[1.5x]`、`剩余流量`），且经常有失联死节点导致网络卡顿；
- **多客户端格式割裂**：Clash (YAML)、Sing-Box (JSON)、Surge (List)、Shadowrocket (Base64) 格式各异，管理成本极高；
- **多用户共享缺乏隐私**：搭建给家人或朋友使用时，管理员能轻易窥探到普通用户的私密节点。

**SubHub (Clash Sub Hub)** 专为解决上述痛点而生，提供一个**开箱即用、极速轻量、安全隔离**的现代化中台服务。

---

## 📊 SubHub vs Sub-Store 深度功能对比

| 功能维度 | 🚀 **SubHub (本项目)** | 📦 **Sub-Store** | 优势说明 |
| :--- | :--- | :--- | :--- |
| **上手门槛与定位** | ⭐️⭐️⭐️⭐️⭐️ **零门槛开箱即用**<br>现代化暗黑 Web GUI，全图形化配置 | ⭐️⭐️⭐️ 偏极客<br>需学习专有算子语法与脚本编程 | SubHub 无论是小白还是资深玩家均可 1 分钟上手 |
| **全局多源流量看板** | ✅ **原生聚合大屏**<br>实时汇聚所有上游 `Subscription-Userinfo`，汇总总配额、已用量、最早到期日 | ❌ **无全局多源看板**<br>仅能单独展示单订阅信息 | SubHub 实时掌握所有资产的流量与续费状态 |
| **通用智能客户端自适应** | ✅ **单一直链 (`/api/sub`)**<br>自动嗅探 User-Agent 智能下发对应格式 | ❌ 需在 URL 显式指定参数<br>`?target=clash` / `?target=sing-box` | 无论什么客户端，只需复制一个通用直链即可 |
| **服务端真机并发测速** | ✅ **原生服务端并发 TCP 探测**<br>支持**自动剔除死节点**与**按真实延迟排序** | ⚠️ 依赖外部脚本或客户端本地探测 | 在服务端下发前就过滤掉挂掉的节点，客户端始终丝滑 |
| **国旗注入与地区定位** | ✅ **全自动流水线**<br>关键字匹配 + **离线 MaxMind GeoIP 定位** + 广告/倍率清洗 | ✅ 支持（需自行配置组合算子） | SubHub 即使节点名无任何地区信息，也能通过 IP 精准识别国旗 |
| **多用户权限与隐私隔离** | ✅ **企业级 RBAC 多租户**<br>管理员**零知识隐私**（不可窥视普通用户节点），支持禁用账号与防爆破锁定 | ❌ 默认单用户模式<br>无细粒度多租户隔离与账号管理 | 适合多人/团队/家庭私有化共用部署 |
| **规则注入与 DNS 防泄漏** | ✅ **内置精细分流引擎**<br>支持域名/IP/关键词/进程分流，内置严格 `fake-ip-filter` 杜绝 DNS 泄漏 | ⚠️ 主要专注于节点处理，分流需额外配合规则集 | SubHub 集成完整的分流与 DNS 优化模板 |
| **全量备份与跨机迁移** | ✅ **一键单文件全局快照备份/恢复**<br>支持用户数据、哈希密码、配置全量无缝还原 | ⚠️ 依赖 Gist 同步或手动拷贝配置文件 | 换服务器或灾备只需 1 秒一键导入恢复 |
| **部署与运行资源** | 🐳 **极轻量 Docker / Node 运行时**<br>内存占用仅 30~50MB，毫秒级响应 | 🐳 Docker / Surge / Loon 模块运行 | 极低资源消耗，超轻量 VPS 亦可流畅运行 |

---

## ✨ 核心功能特性

### 1. 🌐 通用智能客户端自适应（Smart Adaptive URL）
对外提供唯一的终身固定订阅直链：
```
https://你的域名/api/sub?token=你的专属Token
```
- **智能嗅探**：服务端根据请求头的 `User-Agent` 自动识别客户端（Clash Verge、Mihomo Party、Sing-Box、Surge、Shadowrocket、Stash、Quantumult X 等），毫秒级下发完美匹配的配置格式；
- **全格式支持**：
  - 🐱 **Clash / Mihomo**：自动输出规范 YAML 配置（包含 Proxy-Groups、Rules、DNS 配置）；
  - 📦 **Sing-Box**：自动生成 `sing-box.json` Outbounds 结构；
  - ⚡ **Surge**：自动生成 `surge.list` 策略组代理列表；
  - 🚀 **通用 Base64**：输出标准 Base64 代理节点列表（适配 Shadowrocket / V2rayN / Loon）；
  - 📜 **JS 规则脚本**：输出 `main(config, profileName)` 预处理覆写脚本。

### 2. 📊 实时多订阅聚合流量看板（Live Quota Dashboard）
- 自动追踪上游 `Subscription-Userinfo` 响应头；
- 汇聚多个 VPS / 机场的总流量配额、已用流量、剩余可用量；
- 智能计算全局**最早到期日**并提供到期倒计时提醒；
- 添加订阅时点击「⚡ 测试并解析」，**秒级自动识别并回填到期日**。

### 3. ⚡ 服务端后台真实测速与死节点剔除（Backend Latency Check）
- **真机并发探测**：服务端主动对所有聚合节点发起高并发 TCP 握手探测（支持自定义超时时间，如 1500ms）；
- **死节点自动剔除**：开启「剔除失联节点」后，超时的故障节点在下发给客户端前将被自动剥离；
- **延迟优先排序**：支持将低延迟的优质节点自动排在列表前列。

### 4. 🇨🇳 国旗 Emoji 智能注入与规范化流水线
- **地区关键字匹配**：支持识别全球数十个主流国家/地区（HK、JP、US、SG、TW、KR、GB、DE、FR 等）；
- **离线 GeoIP 兜底定位**：当节点名称无地区信息时，自动调用 MaxMind GeoLite 离线 IP 库，根据节点域名解析 IP 进行物理归属地定位并注入国旗；
- **广告与倍率清洗**：自动去除 `[1.5x]`、`0.2倍率`、`剩余流量: 200G`、`官网地址` 等视觉干扰；
- **自定义正则重命名**：提供可视化的正则替换规则列表与实时重命名效果预览。

### 5. 🛡️ 双模分流规则引擎与 DNS 防泄漏优化
- **GUI 可视化编排**：按域名、关键词、IP-CIDR、进程名（Mac/Win）一键增删分流规则；
- **Monaco/CodeMirror 在线代码编辑**：支持纯手写 JS 脚本，享受全自由的配置操控；
- **企业级 DNS 优化**：内置阿里 DNS、腾讯 DNSPod 与 Cloudflare DoH，配备完善的国内大厂 SDK / 银行应用 `fake-ip-filter` 排除名单，彻底告别 DNS 泄漏与国内应用风控。

### 6. 🔐 企业级多租户隔离与零知识隐私安全
- **严格权限隔离**：每个用户拥有完全独立的规则集、上游订阅与专属 Token；
- **管理员零知识隐私**：管理员在后台**无法查看任何普通用户的私有节点 URL 与订阅详情**；
- **防暴力破解与时序攻击保护**：
  - 5 次连续密码错误自动触发账户锁定 15 分钟（HTTP 429）；
  - 针对非存在用户引入固定时长 Dummy Hash 比较，彻底免疫时序探测攻击；
- **账号状态控制**：管理员可一键「禁用/启用」违规账户，被禁用户立即销毁 Session 并 403 阻断所有订阅请求。

### 7. 📦 系统全局一键快照备份与跨机迁移
- **单文件全量快照**：管理员可在后台一键导出包含所有用户账户、加密哈希与私有配置的全局快照（`subhub_system_backup_*.json`）；
- **无感秒级还原**：新机器一键导入，系统自动原子级写入存储并热重载，无需停机。

---

## 🛠️ 多方式极速部署指南

无论你是使用海外 VPS、家用服务器、群晖/威联通 NAS，还是各类面板工具，SubHub 均提供最简化的部署路径。

---

### 方式一：Linux VPS 极速一键管理脚本（强烈推荐）

适用于 **Ubuntu / Debian / CentOS / Alpine / RockyLinux** 等主流发行版，自动检测并安装 Docker，内置交互式运维菜单：

```bash
# 运行一键交互式管理脚本
bash <(curl -fsSL https://raw.githubusercontent.com/wm1634208243/sub-hub/main/install.sh)
```

> **💡 脚本支持快捷命令**：
> - `bash install.sh install`：全新安装
> - `bash install.sh update`：无损更新升级至最新版本
> - `bash install.sh restart`：重启服务
> - `bash install.sh logs`：实时追踪日志
> - `bash install.sh backup`：一键全量打包备份

---

### 方式二：Docker Compose 部署（标准容器化）

1. 创建项目目录：
   ```bash
   mkdir -p /opt/subhub && cd /opt/subhub
   ```

2. 编写 `docker-compose.yml` 文件：
   ```yaml
   version: '3.8'

   services:
     subhub:
       image: node:20-alpine
       container_name: subhub
       restart: always
       working_dir: /app
       ports:
         - "3000:3000"
       volumes:
         - ./data:/app/data
         - .:/app
       environment:
         - PORT=3000
         - NODE_ENV=production
       command: sh -c "npm install --production && node server.js"
   ```

3. 启动并放入后台运行：
   ```bash
   docker compose up -d
   ```

4. 访问面板：浏览器打开 `http://你的服务器IP:3000`
   - **初始管理员账号**：`admin`
   - **初始管理员密码**：`admin`（登录后请立即在后台修改！）

---

### 方式三：Docker CLI 单行命令极速运行

无需克隆仓库，直接单行命令启动容器并映射持久化目录：

```bash
mkdir -p /opt/subhub/data

docker run -d \
  --name subhub \
  --restart always \
  -p 3000:3000 \
  -v /opt/subhub/data:/app/data \
  -e PORT=3000 \
  -e NODE_ENV=production \
  ghcr.io/wm1634208243/sub-hub:latest
```

---

### 方式四：1Panel / Portainer / 群晖 NAS / CasaOS 部署

- **1Panel / Portainer**：
  - 进入「容器」➔「创建容器/编排」；
  - 端口映射：`3000:3000`；
  - 目录挂载（Volume）：`/opt/subhub/data` ➔ `/app/data`；
  - 环境变量：`NODE_ENV=production`。
- **群晖 Synology Container Manager**：
  - 搜索 `node:20-alpine` 镜像；
  - 容器卷选择本地共享文件夹（如 `/docker/subhub/data`）映射至容器内的 `/app/data`；
  - 本地端口填写 `3000`，容器端口填写 `3000` 即可。

---

### 方式五：Node.js 原生运行 / PM2 / Systemd 守护

要求系统安装有 **Node.js 18.0+**：

```bash
# 1. 克隆代码仓库
git clone https://github.com/wm1634208243/sub-hub.git
cd sub-hub

# 2. 安装依赖并启动
npm install --production

# 3. 生产环境推荐使用 PM2 进行多核与开机自启管理
npm install -g pm2
pm2 start server.js --name "subhub"
pm2 save
pm2 startup
```

> **可选：使用 Systemd 守护进程**：
> 创建服务文件 `/etc/systemd/system/subhub.service`：
> ```ini
> [Unit]
> Description=SubHub Service
> After=network.target
> 
> [Service]
> Type=simple
> User=root
> WorkingDirectory=/opt/subhub
> ExecStart=/usr/bin/node /opt/subhub/server.js
> Restart=on-failure
> Environment=PORT=3000 NODE_ENV=production
> 
> [Install]
> WantedBy=multi-user.target
> ```
> 启用命令：`systemctl daemon-reload && systemctl enable --now subhub`

---

## 🌐 域名绑定与 HTTPS 配置示例

### 1. Nginx 反向代理配置（以 `sub.yourdomain.com` 为例）：

```nginx
server {
    listen 80;
    server_name sub.yourdomain.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name sub.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/sub.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/sub.yourdomain.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket 支持 (如需)
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### 2. Caddy 2 自动化配置（极简自动申请 SSL）：

编辑 `/etc/caddy/Caddyfile`：

```caddy
sub.yourdomain.com {
    reverse_proxy 127.0.0.1:3000
}
```

重载 Caddy：`caddy reload`

---

## 📱 全平台客户端接入指南

登录 SubHub 面板后，在首页顶部点击 **「复制通用订阅直链」**（形如 `https://sub.yourdomain.com/api/sub?token=xxx`）：

| 客户端平台 | 推荐软件 | 接入方法 |
| :--- | :--- | :--- |
| **Windows / macOS / Linux** | **Clash Verge Rev** / **Mihomo Party** | 打开软件 ➔ 订阅 (Profiles) ➔ 新建订阅 ➔ 粘贴直链 URL ➔ 保存并更新 |
| **iOS / iPadOS** | **Shadowrocket** / **Stash** / **Loon** | 点击右上角 `+` ➔ 类型选择 `Subscribe` / 订阅 ➔ 粘贴直链 URL ➔ 完成 |
| **Android** | **Clash Meta for Android** / **Sing-box SFA** | 配置 ➔ 新建配置 ➔ 从 URL 导入 ➔ 粘贴直链 URL ➔ 保存并下载 |
| **Surge (iOS / Mac)** | **Surge 5** | 策略组 ➔ 外部代理列表 (Policy List) ➔ 填入 `https://.../api/surge.list?token=xxx` |
| **Sing-Box 原生内核** | **Sing-Box** | 在配置 `endpoints` / `outbounds` 中引用 `https://.../api/sing-box.json?token=xxx` |

---

## 🔌 开放订阅端点一览

| 端点 URL | 说明 | 适用场景 |
| :--- | :--- | :--- |
| `GET /api/sub?token=xxx` | **🌟 智能自适应全能直链** | 根据 User-Agent 自动分发对应格式（强烈推荐） |
| `GET /api/clash.yaml?token=xxx` | 专享 Clash / Mihomo YAML 配置 | Clash Verge, Clash Nyanpasu, Clash Meta |
| `GET /api/sing-box.json?token=xxx` | 专享 Sing-Box Outbounds JSON 配置 | Sing-Box GUI / 命令行客户端 |
| `GET /api/surge.list?token=xxx` | 专享 Surge 策略列表格式 | Surge iOS / macOS 外部策略组 |
| `GET /api/base64?token=xxx` | 专享标准 Base64 节点列表 | Shadowrocket, V2rayN, Quantumult X, Loon |
| `GET /api/rules.js?token=xxx` | 专享 JS 覆写预处理脚本 | Clash Verge 扩展脚本 / Stash JavaScript 覆写 |

---

## 📁 目录架构概览

```
rule-hub/
├── server.js               # Express 核心 API 服务、路由分发与安全防护
├── aggregator.js           # 节点多源聚合、去重与格式标准化引擎
├── subscription-fetcher.js # 上游订阅拉取器、缓存管理与 Userinfo 配额解析
├── node-renamer.js         # 国旗 Emoji 注入、GeoIP 离线库定位与正则清洗流水线
├── latency-tester.js       # 服务端高并发真机 TCP 探测与死节点过滤引擎
├── format-converter.js     # Clash / Sing-Box / Surge / Base64 多格式转换器
├── compiler.js             # 规则编译与预处理 JS 脚本生成器
├── public/                 # 前端暗黑科技风 SPA 页面 (Vue 3 + TailwindCSS + CodeMirror)
│   └── index.html
├── data/                   # 持久化数据存储目录 (自动挂载持久卷)
│   ├── users.json          # 用户账户与角色数据 (Bcrypt 加密存储)
│   ├── configs/            # 各用户的独立规则与订阅配置文件
│   └── sessions.json       # 用户登录态 Session
├── Dockerfile              # Docker 镜像构建配置
└── docker-compose.yml      # 一键容器化编排文件
```

---

## 🔒 安全与免责声明

1. 本项目仅供学习、网络运维、个人多设备配置同步与合规的网络管理使用；
2. 请勿将本项目用于任何违反当地法律法规的活动；
3. 部署于公网时，请务必开启 HTTPS，并第一时间修改默认管理员密码 `admin`。

---

## 📄 License

本项目采用 [MIT License](LICENSE) 开源协议，欢迎自由 Fork、Star 与提交 PR！
