import {
  buildConversationContext,
  buildConversationRetrievalContext,
  saveConversation,
  searchConversationHistory,
  type Conversation,
} from "../src/services/conversations";
import { buildKnowledgeContext } from "../src/services/knowledge";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function check(name: string, run: () => void): void {
  try {
    run();
    console.log(`✓ ${name}`);
  } catch (error) {
    console.error(`✗ ${name}: ${error instanceof Error ? error.message : String(error)}`);
    throw error;
  }
}

const storage = new Map<string, string>();
Object.assign(globalThis, {
  localStorage: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  },
  window: { dispatchEvent: () => true },
  Event: class Event { constructor(public type: string) {} },
});

function conversation(id: string, projectId: string | undefined, content: string): Conversation {
  return {
    id,
    projectId,
    title: `Conversation ${id}`,
    messages: [{
      id: `${id}-message`,
      role: "user",
      content,
      createdAt: "2026-10-11T00:00:00Z",
    }],
    createdAt: "2026-10-11T00:00:00Z",
    updatedAt: "2026-10-11T00:00:00Z",
  };
}

check("complete history search is isolated by project", () => {
  saveConversation(conversation("a", "project-a", "Decision: budget is 128000 AUD"));
  saveConversation(conversation("b", "project-b", "Decision: budget is 64000 AUD"));
  const hits = searchConversationHistory("budget AUD", { projectId: "project-a" });
  assert(hits.length === 1, "search leaked across project boundaries");
  assert(hits[0].conversationId === "a", "wrong project history was retrieved");
  assert(hits[0].messageId === "a-message", "message provenance was lost");
});

check("retrieved conversation evidence carries source identities", () => {
  const context = buildConversationRetrievalContext("budget AUD", {
    projectId: "project-a",
    tokenBudget: 512,
  });
  assert(context?.content.includes("conversation:a; message:a-message"), "history provenance missing");
});

check("unassigned conversations do not retrieve project history", () => {
  saveConversation(conversation("unassigned", undefined, "Private unassigned launch note"));
  const context = buildConversationRetrievalContext("budget AUD", {
    projectId: null,
    tokenBudget: 512,
  });
  assert(!context?.content.includes("conversation:a"), "project history leaked into an unassigned chat");
});

check("compaction protects decisions and numbers with message provenance", () => {
  const messages = Array.from({ length: 18 }, (_, index) => ({
    id: `turn-${index + 1}`,
    role: index % 2 === 0 ? "user" as const : "assistant" as const,
    content:
      index === 2
        ? `Decision: the approved budget is 128000 AUD. ${"context ".repeat(180)}`
        : `${index} ${"ordinary context ".repeat(180)}`,
    createdAt: `2026-10-11T00:00:${String(index).padStart(2, "0")}Z`,
  }));
  const projected = buildConversationContext({
    id: "long",
    projectId: "project-a",
    title: "Long",
    messages,
    createdAt: messages[0].createdAt,
    updatedAt: messages.at(-1)!.createdAt,
  }, {
    contextWindowTokens: 4_096,
    outputReserveTokens: 1_024,
  });
  assert(projected.compactedMessageIds.includes("turn-3"), "fixture did not compact the decision");
  assert(projected.summary?.includes("turn-3"), "decision source id was dropped");
  assert(projected.summary?.includes("128000 AUD"), "important number was dropped");
});

check("project knowledge context is bounded and source traceable", () => {
  const context = buildKnowledgeContext({
    id: "project-a",
    name: "Launch",
    createdAt: "2026-10-11T00:00:00Z",
    updatedAt: "2026-10-11T00:00:00Z",
  }, [{
    sourceId: "source-1",
    sourceName: "plan.md",
    sourcePath: "/tmp/plan.md",
    kind: "text",
    locator: "line:7",
    excerpt: "The release gate requires all local acceptance checks to pass.",
    score: 3,
  }], 256);
  assert(context?.content.includes("source:source-1; line:7; plan.md"), "source citation missing");
  assert(context?.content.includes("only the retrieved excerpts"), "grounding boundary missing");
});

console.log("PASS Beta.4 knowledge engine behavior");
