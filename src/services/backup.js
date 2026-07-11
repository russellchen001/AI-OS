import { invoke } from "@tauri-apps/api/core";
export async function createBackup(request) {
    return invoke("create_backup", {
        request,
    });
}
export async function restoreBackup(request) {
    return invoke("restore_backup", {
        request,
    });
}
export async function listBackups(directory) {
    if (!directory.trim()) {
        return [];
    }
    return invoke("list_backups", {
        directory,
    });
}
export async function revealBackup(archivePath) {
    return invoke("reveal_backup", {
        archivePath,
    });
}
export async function deleteBackup(archivePath) {
    return invoke("delete_backup", {
        archivePath,
    });
}
