import { invoke } from "@tauri-apps/api/core";

import type {
  OllamaModel,
  OllamaPullProgress,
} from "../types/index";

export type OmlxAdminModel = {
  name: string;
  displayName: string;
  size: number;
  sizeFormatted: string;
};

export async function listOmlxAdminModels(): Promise<OmlxAdminModel[]> {
  return invoke<OmlxAdminModel[]>("list_omlx_admin_models");
}

export async function showOmlxModel(model: string): Promise<OmlxAdminModel> {
  return invoke<OmlxAdminModel>("show_omlx_model", { model });
}

export async function showOmlxModelInFinder(model: string): Promise<void> {
  return invoke<void>("show_omlx_model_in_finder", { model });
}

export async function deleteOmlxModel(model: string): Promise<string> {
  return invoke<string>("delete_omlx_model", { model });
}

export async function pullOmlxModel(repoId: string): Promise<string> {
  return invoke<string>("pull_omlx_model", { repoId });
}

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

export async function showOllamaModelInFinder(model: string): Promise<void> {
  return invoke<void>("show_ollama_model_in_finder", { model });
}
