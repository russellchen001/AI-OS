export const APP_INFO = {
    name: "AI OS",
    version: "1.2.0",
    description: "Personal AI Workspace",
    identifier: "com.russellchen.aios",
};
export const STORAGE_KEYS = {
    settings: "ai-os-settings",
    mcpServers: "ai-os-mcp-servers",
    backupHistory: "ai-os-backup-history",
};
export const INITIAL_SERVICES = [
    {
        name: "OpenClaw",
        icon: "🤖",
        description: "Local AI gateway",
        status: "Unknown",
    },
    {
        name: "Ollama",
        icon: "🦙",
        description: "Local model runtime",
        status: "Unknown",
    },
    {
        name: "Docker",
        icon: "🐳",
        description: "Container runtime",
        status: "Unknown",
    },
    {
        name: "Open WebUI",
        icon: "🌐",
        description: "Browser AI workspace",
        status: "Unknown",
    },
    {
        name: "Cherry Studio",
        icon: "🍒",
        description: "Desktop AI client",
        status: "Unknown",
    },
];
export const DEFAULT_SETTINGS = {
    refreshInterval: 5,
    openClawUrl: "http://localhost:18789",
    ollamaUrl: "http://localhost:11434",
    openWebUiUrl: "http://localhost:3000",
    theme: "dark",
    backupDirectory: "",
    includeOpenClawConfig: true,
    includeAiOsSettings: true,
    logLineLimit: 500,
};
export const EMPTY_METRICS = {
    cpu: 0,
    memoryUsed: 0,
    memoryTotal: 0,
    diskUsed: 0,
    diskTotal: 0,
};
export const DEFAULT_MCP_SERVERS = [
    {
        id: "filesystem",
        name: "Filesystem",
        description: "Read and manage approved local files",
        enabled: false,
        transport: "stdio",
        command: "npx",
        args: [
            "-y",
            "@modelcontextprotocol/server-filesystem",
            "~",
        ],
        environment: {},
    },
    {
        id: "github",
        name: "GitHub",
        description: "Access repositories, issues and pull requests",
        enabled: false,
        transport: "stdio",
        command: "npx",
        args: [
            "-y",
            "@modelcontextprotocol/server-github",
        ],
        environment: {
            GITHUB_PERSONAL_ACCESS_TOKEN: "",
        },
    },
];
export const LOG_LINE_LIMIT_OPTIONS = [
    100,
    250,
    500,
    1000,
    2000,
];
export const REFRESH_INTERVAL_OPTIONS = [
    2,
    5,
    10,
    30,
    60,
];
export const POPULAR_OLLAMA_MODELS = [
    {
        name: "llama3.2:3b",
        description: "Fast general-purpose model",
    },
    {
        name: "qwen2.5:7b",
        description: "Strong multilingual assistant",
    },
    {
        name: "deepseek-r1:7b",
        description: "Reasoning-focused model",
    },
    {
        name: "gemma3:4b",
        description: "Compact Google model",
    },
    {
        name: "nomic-embed-text",
        description: "Text embedding model",
    },
];
