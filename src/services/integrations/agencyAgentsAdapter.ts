import type {
  AgencyAgentDefinition,
  AgencyAgentsCatalog,
  AgencyAgentsDivision,
  AgencyAgentsRunbook,
  CouncilIntegrationProbe,
} from "../../types/councilIntegrations";

export const AGENCY_AGENTS_SOURCE_COMMIT =
  "ad9264e309bd5e5422c04784372d7841b1e5d604";

const RAW_BASE = `https://raw.githubusercontent.com/msitarzewski/agency-agents/${AGENCY_AGENTS_SOURCE_COMMIT}`;
const TREE_URL = `https://api.github.com/repos/msitarzewski/agency-agents/git/trees/${AGENCY_AGENTS_SOURCE_COMMIT}?recursive=1`;

async function fetchJson(path: string): Promise<unknown> {
  const response = await fetch(`${RAW_BASE}/${path.replace(/^\/+/, "")}`, {
    headers: {
      Accept: "application/json,text/plain",
    },
  });

  if (!response.ok) {
    throw new Error(`Agency Agents ${path}: HTTP ${response.status}`);
  }

  return response.json();
}

async function fetchText(path: string): Promise<string> {
  const response = await fetch(`${RAW_BASE}/${path.replace(/^\/+/, "")}`, {
    headers: {
      Accept: "text/plain",
    },
  });

  if (!response.ok) {
    throw new Error(`Agency Agents ${path}: HTTP ${response.status}`);
  }

  return response.text();
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function collectRunbookAgents(value: unknown): string[] {
  const found = new Set<string>();

  const walk = (node: unknown): void => {
    if (Array.isArray(node)) {
      node.forEach(walk);
      return;
    }

    const record = asRecord(node);
    if (!record) {
      return;
    }

    if (Array.isArray(record.agents)) {
      for (const agent of record.agents) {
        if (typeof agent === "string" && agent.trim()) {
          found.add(agent.trim());
        }
      }
    }

    Object.values(record).forEach(walk);
  };

  walk(value);
  return [...found];
}

function normalizeDivisions(value: unknown): AgencyAgentsDivision[] {
  const record = asRecord(value);
  if (!record) {
    return [];
  }

  const divisionRecord = asRecord(record.divisions);
  const source = Array.isArray(record.divisions)
    ? record.divisions
    : divisionRecord
      ? Object.entries(divisionRecord).map(([id, raw]) => ({ id, raw }))
      : Object.entries(record)
        .filter(([key]) => !key.startsWith("_"))
        .map(([id, raw]) => ({
          id,
          raw,
        }));

  return source.map((item, index) => {
    const itemRecord = asRecord(item);

    if (itemRecord && "raw" in itemRecord && "id" in itemRecord) {
      const raw = asRecord(itemRecord.raw) ?? {};

      const id =
        typeof itemRecord.id === "string" ? itemRecord.id : `division-${index}`;

      return {
        id,
        name: typeof raw.name === "string" ? raw.name : id,
        description:
          typeof raw.description === "string" ? raw.description : undefined,
        raw,
      };
    }

    const raw = itemRecord ?? {};
    const id =
      typeof raw.id === "string"
        ? raw.id
        : typeof raw.slug === "string"
          ? raw.slug
          : `division-${index}`;

    return {
      id,
      name: typeof raw.name === "string" ? raw.name : id,
      description:
        typeof raw.description === "string" ? raw.description : undefined,
      raw,
    };
  });
}

function normalizeAgentDefinitions(
  value: unknown,
  divisions: AgencyAgentsDivision[],
): AgencyAgentDefinition[] {
  const record = asRecord(value);
  const tree = Array.isArray(record?.tree) ? record.tree : [];
  const divisionIds = new Set(divisions.map((division) => division.id));

  return tree
    .map(asRecord)
    .filter((item): item is Record<string, unknown> => item !== null)
    .map((item) => (typeof item.path === "string" ? item.path : ""))
    .filter((path) => {
      const [division, ...rest] = path.split("/");
      return (
        divisionIds.has(division) &&
        rest.length === 1 &&
        rest[0].endsWith(".md") &&
        !/^readme\.md$/i.test(rest[0])
      );
    })
    .map((path) => {
      const [division, filename] = path.split("/");
      return {
        division,
        slug: filename.replace(/\.md$/i, ""),
        path,
      };
    });
}

function normalizeRunbooks(value: unknown): AgencyAgentsRunbook[] {
  const record = asRecord(value);
  if (!record) {
    return [];
  }

  const rawRunbooks = Array.isArray(record.runbooks)
    ? record.runbooks
    : Object.entries(record)
        .filter(([key]) => !key.startsWith("_"))
        .map(([id, raw]) => ({
          id,
          raw,
        }));

  return rawRunbooks.map((item, index) => {
    const wrapper = asRecord(item);

    if (wrapper && "raw" in wrapper && "id" in wrapper) {
      const raw = asRecord(wrapper.raw) ?? {};

      const id =
        typeof wrapper.id === "string" ? wrapper.id : `runbook-${index}`;

      return {
        id,
        name:
          typeof raw.name === "string"
            ? raw.name
            : typeof raw.title === "string"
              ? raw.title
              : id,
        mode: typeof raw.mode === "string" ? raw.mode : undefined,
        agents: collectRunbookAgents(raw),
        raw,
      };
    }

    const raw = wrapper ?? {};
    const id =
      typeof raw.id === "string"
        ? raw.id
        : typeof raw.slug === "string"
          ? raw.slug
          : `runbook-${index}`;

    return {
      id,
      name:
        typeof raw.name === "string"
          ? raw.name
          : typeof raw.title === "string"
            ? raw.title
            : id,
      mode: typeof raw.mode === "string" ? raw.mode : undefined,
      agents: collectRunbookAgents(raw),
      raw,
    };
  });
}

export class AgencyAgentsAdapter {
  readonly id = "agency-agents" as const;
  readonly name = "Agency Agents";

  async probe(): Promise<CouncilIntegrationProbe> {
    const checkedAt = new Date().toISOString();

    try {
      const [divisions, runbooks] = await Promise.all([
        fetchJson("divisions.json"),
        fetchJson("strategy/runbooks.json"),
      ]);

      return {
        id: this.id,
        name: this.name,
        status: "available",
        detail: "Agency Agents catalog and runbooks are reachable.",
        checkedAt,
        metadata: {
          sourceCommit: AGENCY_AGENTS_SOURCE_COMMIT,
          divisionCount: normalizeDivisions(divisions).length,
          runbookCount: normalizeRunbooks(runbooks).length,
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
          sourceCommit: AGENCY_AGENTS_SOURCE_COMMIT,
        },
      };
    }
  }

  async loadCatalog(): Promise<AgencyAgentsCatalog> {
    const [divisionsRaw, runbooksRaw, treeRaw] = await Promise.all([
      fetchJson("divisions.json"),
      fetchJson("strategy/runbooks.json"),
      fetch(TREE_URL, { headers: { Accept: "application/vnd.github+json" } }).then(
        async (response) => {
          if (!response.ok) {
            throw new Error(`Agency Agents tree: HTTP ${response.status}`);
          }
          return response.json();
        },
      ),
    ]);
    const divisions = normalizeDivisions(divisionsRaw);

    return {
      commit: AGENCY_AGENTS_SOURCE_COMMIT,
      divisions,
      runbooks: normalizeRunbooks(runbooksRaw),
      agents: normalizeAgentDefinitions(treeRaw, divisions),
    };
  }

  async loadAgent(definition: AgencyAgentDefinition): Promise<string> {
    return fetchText(definition.path);
  }

  agentPath(division: string, slug: string): AgencyAgentDefinition {
    return {
      division,
      slug,
      path: `${division}/${slug}.md`,
    };
  }
}
