/**
 * presets.js
 * Standard scenario rule presets & domain/IP databases for SubHub
 */

export const PRESET_SCENARIOS = {
  ai: {
    id: 'enableAiGroup',
    name: '🤖 AI 专线',
    desc: 'ChatGPT, Claude, Gemini, Copilot 等全套 AI 专线',
    defaultAction: '🚀 节点选择',
    domains: [
      'openai.com',
      'chatgpt.com',
      'ai.com',
      'oaistatic.com',
      'oaiusercontent.com',
      'anthropic.com',
      'claude.ai',
      'gemini.google.com',
      'copilot.microsoft.com',
      'groq.com',
      'mistral.ai',
      'perplexity.ai',
      'sora.com',
      'poe.com',
      'v0.dev',
      'cohere.com',
      'deepseek.com'
    ],
    keywords: [
      'openai',
      'anthropic',
      'claude',
      'chatgpt',
      'perplexity',
      'deepseek'
    ]
  },

  media: {
    id: 'enableMediaGroup',
    name: '🎬 国际流媒体',
    desc: 'YouTube, Netflix, Disney+, Spotify, TikTok, HBO 等',
    defaultAction: '⚡ 自动优选',
    domains: [
      'youtube.com',
      'googlevideo.com',
      'ytimg.com',
      'youtu.be',
      'netflix.com',
      'netflix.net',
      'nflxvideo.net',
      'nflximg.net',
      'nflxext.com',
      'disneyplus.com',
      'disney-portal.my.onetrust',
      'dssott.com',
      'disneystreaming.com',
      'spotify.com',
      'scdn.co',
      'spoti.fi',
      'tiktok.com',
      'byteoversea.com',
      'ibytedtos.com',
      'hbo.com',
      'hbomax.com',
      'max.com',
      'twitch.tv',
      'ttvnw.net',
      'bilibili.tv',
      'primevideo.com',
      'amazonvideo.com'
    ],
    keywords: [
      'netflix',
      'spotify',
      'youtube',
      'disneyplus'
    ]
  },

  telegram: {
    id: 'enableTelegramGroup',
    name: '📲 Telegram',
    desc: 'Telegram 官方全域名及专用 IP 段',
    defaultAction: '🚀 节点选择',
    domains: [
      'telegram.org',
      't.me',
      'telegram.me',
      'telegram-cdn.org',
      'telegra.ph',
      'tdesktop.com',
      'telesco.pe'
    ],
    ips: [
      '91.108.4.0/22',
      '91.108.8.0/22',
      '91.108.12.0/22',
      '91.108.16.0/22',
      '91.108.20.0/22',
      '91.108.56.0/22',
      '149.154.160.0/20',
      '149.154.164.0/22',
      '149.154.168.0/22',
      '149.154.172.0/22',
      '185.76.151.0/24'
    ]
  },

  games: {
    id: 'enableGameGroup',
    name: '🎮 游戏平台',
    desc: 'Steam, Epic, EA, Riot, Blizzard, PSN 等游戏平台',
    defaultAction: 'DIRECT',
    domains: [
      'steampowered.com',
      'steamcommunity.com',
      'steamgames.com',
      'steamusercontent.com',
      'steamcontent.com',
      'steamserver.net',
      'epicgames.com',
      'ea.com',
      'origin.com',
      'riotgames.com',
      'leagueoflegends.com',
      'valorgame.com',
      'blizzard.com',
      'battle.net',
      'playstation.com',
      'playstation.net',
      'sonyentertainmentnetwork.com',
      'xbox.com',
      'xboxlive.com',
      'ubisoft.com',
      'uplay.com',
      'gog.com'
    ]
  },

  apple: {
    id: 'enableAppleGroup',
    name: '🍎 Apple / 微软',
    desc: 'App Store, iCloud, Windows Update, GitHub 等',
    defaultAction: 'DIRECT',
    domains: [
      'apple.com',
      'icloud.com',
      'itunes.com',
      'mzstatic.com',
      'apple-dns.net',
      'microsoft.com',
      'windows.com',
      'windowsupdate.com',
      'live.com',
      'office.com',
      'onedrive.com',
      'github.com',
      'githubusercontent.com'
    ]
  },

  adblock: {
    id: 'enableAdBlock',
    name: '🛑 广告拦截',
    desc: '国内外常见广告追踪与数据遥测 (REJECT)',
    defaultAction: 'REJECT',
    domains: [
      'adservice.google.com',
      'googleadservices.com',
      'doubleclick.net',
      'googlesyndication.com',
      'pagead2.googlesyndication.com',
      'ads.twitter.com',
      'ads-twitter.com',
      'adservice.sigmob.cn',
      'pos.baidu.com',
      'union.baidu.com',
      'cpro.baidu.com',
      'adcolony.com',
      'unityads.unity3d.com',
      'vungle.com',
      'applovin.com'
    ],
    keywords: [
      'adservice',
      'adserver',
      'telemetry'
    ]
  }
};
