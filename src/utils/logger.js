export function createLogId() {
    return `${Date.now()}-${Math.random()
        .toString(16)
        .slice(2)}`;
}
export function currentTime() {
    return new Date().toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    });
}
export function createLogEntry(level, message) {
    return {
        id: createLogId(),
        timestamp: currentTime(),
        level,
        message,
    };
}
export function prependLog(logs, level, message, limit = 500) {
    return [
        createLogEntry(level, message),
        ...logs,
    ].slice(0, limit);
}
