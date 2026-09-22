import type {
  CouncilIntegrationProbe,
  LincoBridgeSession,
} from "../../types/councilIntegrations";

export const LINCO_BRIDGE_SOURCE_COMMIT =
  "3a7375858eb17de3db45cd9896578d371c6df9be";

const DEFAULT_LINCO_API = "http://127.0.0.1:3300";

function normalizeBaseUrl(value?: string): string {
  return (
    value?.trim() ||
    localStorage.getItem("ai-os.integrations.lincoBridge.apiBase") ||
    DEFAULT_LINCO_API
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

function unwrapData(value: unknown): unknown {
  const record = asRecord(value);
  return record && "data" in record ? record.data : value;
}

function extractArray(value: unknown): unknown[] {
  const unwrapped = unwrapData(value);

  if (Array.isArray(unwrapped)) {
    return unwrapped;
  }

  const record = asRecord(unwrapped);

  if (!record) {
    return [];
  }

  for (const key of ["sessions", "items", "results"]) {
    const candidate = record[key];

    if (Array.isArray(candidate)) {
      return candidate;
    }
  }

  return [];
}

export class LincoBridgeAdapter {
  readonly id = "linco-bridge" as const;
  readonly name = "Linco Bridge";
  private visitorSessionToken?: string;
  private visitorSessionRequest?: Promise<string>;

  constructor(readonly apiBase = normalizeBaseUrl()) {}

  saveApiBase(): void {
    localStorage.setItem(
      "ai-os.integrations.lincoBridge.apiBase",
      this.apiBase,
    );
  }

  private async ensureVisitorSession(): Promise<string> {
    if (this.visitorSessionToken) return this.visitorSessionToken;
    if (!this.visitorSessionRequest) {
      this.visitorSessionRequest = fetchJson(
        `${this.apiBase}/api/visitor/bootstrap`,
        { method: "POST" },
      )
        .then((payload) => {
          const data = asRecord(unwrapData(payload));
          const token = data?.sessionToken;
          if (typeof token !== "string" || !token.trim()) {
            throw new Error("Linco Bridge returned no visitor session token.");
          }
          this.visitorSessionToken = token;
          return token;
        })
        .finally(() => {
          this.visitorSessionRequest = undefined;
        });
    }
    return this.visitorSessionRequest;
  }

  private async fetchVisitorJson(
    path: string,
    init: RequestInit = {},
    retry = true,
  ): Promise<unknown> {
    const token = await this.ensureVisitorSession();
    try {
      return await fetchJson(`${this.apiBase}${path}`, {
        ...init,
        headers: {
          "X-Linco-Visitor-Session": token,
          ...(init.body ? { "Content-Type": "application/json" } : {}),
          ...(init.headers ?? {}),
        },
      });
    } catch (error) {
      if (retry && error instanceof Error && error.message.startsWith("HTTP 401")) {
        this.visitorSessionToken = undefined;
        return this.fetchVisitorJson(path, init, false);
      }
      throw error;
    }
  }

  async probe(): Promise<CouncilIntegrationProbe> {
    const checkedAt = new Date().toISOString();

    try {
      const config = await fetchJson(`${this.apiBase}/api/demo-config`);

      return {
        id: this.id,
        name: this.name,
        status: "available",
        detail: "Linco Bridge API is reachable.",
        checkedAt,
        metadata: {
          apiBase: this.apiBase,
          sourceCommit: LINCO_BRIDGE_SOURCE_COMMIT,
          config: unwrapData(config),
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
          sourceCommit: LINCO_BRIDGE_SOURCE_COMMIT,
        },
      };
    }
  }

  async listSessions(): Promise<LincoBridgeSession[]> {
    const payload = await this.fetchVisitorJson("/api/sessions");

    return extractArray(payload)
      .map(asRecord)
      .filter((item): item is Record<string, unknown> => item !== null)
      .map((item) => {
        const idCandidate = item.id ?? item.sessionId ?? item.session_id;

        return {
          id: typeof idCandidate === "string" ? idCandidate : "",
          raw: item,
        };
      })
      .filter((session) => session.id.length > 0);
  }

  async resumeSession(sessionId: string): Promise<unknown> {
    return unwrapData(
      await this.fetchVisitorJson(
        `/api/sessions/${encodeURIComponent(sessionId)}/resume`,
        { method: "POST" },
      ),
    );
  }

  async createConversation(
    type: string,
    input: Record<string, unknown> = {},
  ): Promise<unknown> {
    const conversationType = type.trim();
    if (!conversationType) throw new Error("Linco conversation type is required.");
    return unwrapData(
      await this.fetchVisitorJson(
        `/api/agent-chat/${encodeURIComponent(conversationType)}/conversations`,
        { method: "POST", body: JSON.stringify(input) },
      ),
    );
  }

  async listMessages(sessionId: string): Promise<unknown[]> {
    return extractArray(
      await this.fetchVisitorJson(
        `/api/sessions/${encodeURIComponent(sessionId)}/messages`,
      ),
    );
  }

  async sendMessage(
    sessionId: string,
    message: Record<string, unknown>,
  ): Promise<unknown> {
    return unwrapData(
      await this.fetchVisitorJson(
        `/api/sessions/${encodeURIComponent(sessionId)}/messages`,
        { method: "POST", body: JSON.stringify(message) },
      ),
    );
  }

  async cancelTurn(sessionId: string): Promise<unknown> {
    return unwrapData(
      await this.fetchVisitorJson(
        `/api/sessions/${encodeURIComponent(sessionId)}/messages/cancel`,
        { method: "POST" },
      ),
    );
  }

  async getOpenClawBridgeStatus(connectionId?: string): Promise<unknown> {
    const query = connectionId?.trim()
      ? `?connectionId=${encodeURIComponent(connectionId.trim())}`
      : "";

    return unwrapData(
      await this.fetchVisitorJson(
        `/api/agent-bridges/openclaw/status${query}`,
      ),
    );
  }
}
