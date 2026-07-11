export const DEFAULT_SETTINGS = {
    refreshInterval: 5,
    theme: "dark",
    openClawUrl: "http://localhost:18789",
    openWebUiUrl: "http://localhost:3000",
    ollamaUrl: "http://localhost:11434",
    autoOpenWebUi: false,
};
export const INITIAL_SERVICES = [
    {
        name: "OpenClaw",
        icon: "🤖",
        description: "Local AI gateway",
        status: "Unknown",
        canOpen: true,
    },
    {
        name: "Ollama",
        icon: "🦙",
        description: "Local model runtime",
        status: "Unknown",
        canOpen: true,
    },
    {
        name: "Docker",
        icon: "🐳",
        description: "Container runtime",
        status: "Unknown",
        canOpen: false,
    },
    {
        name: "Open WebUI",
        icon: "🌐",
        description: "Browser AI workspace",
        status: "Unknown",
        canOpen: true,
    },
    {
        name: "Cherry Studio",
        icon: "🍒",
        description: "Desktop AI client",
        status: "Unknown",
        canOpen: true,
    },
];
export const NAV_ITEMS = [
    { name: "Dashboard", icon: "🏠" },
    { name: "Services", icon: "🚀" },
    { name: "Backup", icon: "💾" },
    { name: "Logs", icon: "📜" },
    { name: "Settings", icon: "⚙️" },
];
export const EMPTY_METRICS = {
    cpuUsage: 0,
    memoryUsedGb: 0,
    memoryTotalGb: 0,
    diskUsedGb: 0,
    diskTotalGb: 0,
};
