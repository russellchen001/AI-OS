import type {
  CouncilIntegrationProbe,
  PaperclipCompany,
} from "../../types/councilIntegrations";

export const PAPERCLIP_SOURCE_COMMIT =
  "b19307758285e22ef3386cac56031f36ed39815d";

const DEFAULT_PAPERCLIP_API = "http://127.0.0.1:3100";

function normalizeBaseUrl(value?: string): string {
  return (
    value?.trim() ||
    localStorage.getItem("ai-os.integrations.paperclip.apiBase") ||
    DEFAULT_PAPERCLIP_API
  ).replace(/\/+$/, "");
}

async function fetchJson(url: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(url, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.headers ?? {}),
    },
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status} ${response.statusText}`);
  }

  return response.json();
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function extractArray(value: unknown): unknown[] {
  if (Array.isArray(value)) {
    return value;
  }

  const record = asRecord(value);
  if (!record) {
    return [];
  }

  for (const key of ["data", "companies", "items", "results"]) {
    const candidate = record[key];
    if (Array.isArray(candidate)) {
      return candidate;
    }
  }

  return [];
}

export class PaperclipAdapter {
  readonly id = "paperclip" as const;
  readonly name = "Paperclip";

  constructor(readonly apiBase = normalizeBaseUrl()) {}

  saveApiBase(): void {
    localStorage.setItem("ai-os.integrations.paperclip.apiBase", this.apiBase);
  }

  async probe(): Promise<CouncilIntegrationProbe> {
    const checkedAt = new Date().toISOString();

    try {
      const health = await fetchJson(`${this.apiBase}/api/health`);

      const record = asRecord(health);
      const version =
        typeof record?.version === "string" ? record.version : undefined;

      return {
        id: this.id,
        name: this.name,
        status: "available",
        version,
        detail: "Paperclip API is reachable.",
        checkedAt,
        metadata: {
          apiBase: this.apiBase,
          sourceCommit: PAPERCLIP_SOURCE_COMMIT,
          health,
        },
      };
    } catch (error) {
      return {
        id: this.id,
        name: this.name,
        status: "unavailable",
        detail: error instanceof Error ? error.message : String(error),
        checkedAt,
        metadata: {
          apiBase: this.apiBase,
          sourceCommit: PAPERCLIP_SOURCE_COMMIT,
        },
      };
    }
  }

  async listCompanies(): Promise<PaperclipCompany[]> {
    const payload = await fetchJson(`${this.apiBase}/api/companies`);

    return extractArray(payload)
      .map(asRecord)
      .filter((item): item is Record<string, unknown> => item !== null)
      .map((item) => ({
        id: typeof item.id === "string" ? item.id : "",
        name: typeof item.name === "string" ? item.name : "Unnamed company",
        raw: item,
      }))
      .filter((item) => item.id.length > 0);
  }
}
