import { invoke } from "@tauri-apps/api/core";
export async function listOllamaModels() {
    return invoke("list_ollama_models");
}
export async function pullOllamaModel(model) {
    return invoke("pull_ollama_model", {
        model,
    });
}
export async function deleteOllamaModel(model) {
    return invoke("delete_ollama_model", {
        model,
    });
}
export async function runOllamaModel(model, prompt) {
    return invoke("run_ollama_model", {
        model,
        prompt,
    });
}
export async function showOllamaModel(model) {
    return invoke("show_ollama_model", {
        model,
    });
}
