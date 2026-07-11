import { invoke } from "@tauri-apps/api/core";

import type {
  OllamaModel,
  OllamaPullProgress,
} from "../types/index";

export async function listOllamaModels(): Promise<
  OllamaModel[]
> {
  return invoke<OllamaModel[]>(
    "list_ollama_models",
  );
}

export async function pullOllamaModel(
  model: string,
): Promise<OllamaPullProgress> {
  return invoke<OllamaPullProgress>(
    "pull_ollama_model",
    {
      model,
    },
  );
}

export async function deleteOllamaModel(
  model: string,
): Promise<string> {
  return invoke<string>(
    "delete_ollama_model",
    {
      model,
    },
  );
}

export async function runOllamaModel(
  model: string,
  prompt: string,
): Promise<string> {
  return invoke<string>(
    "run_ollama_model",
    {
      model,
      prompt,
    },
  );
}

export async function showOllamaModel(
  model: string,
): Promise<string> {
  return invoke<string>(
    "show_ollama_model",
    {
      model,
    },
  );
}