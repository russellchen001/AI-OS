import { deriveAiCenterContextLimits } from "../src/services/aiCenter";
import {
  buildConversationContext,
  estimateContextTokens,
  type Conversation,
} from "../src/services/conversations";

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

function conversationWith(contents: string[]): Conversation {
  return {
    id: "conversation-beta4",
    title: "Beta.4 context test",
    messages: contents.map((content, index) => ({
      id: `message-${index + 1}`,
      role: index % 2 === 0 ? "user" : "assistant",
      content,
      createdAt: `2026-10-11T00:00:${String(index).padStart(2, "0")}Z`,
    })),
    createdAt: "2026-10-11T00:00:00Z",
    updatedAt: "2026-10-11T00:00:10Z",
  };
}

check("manual model limit controls the context window", () => {
  const limits = deriveAiCenterContextLimits([{ contextWindow: 128_000 }]);
  assert(limits.contextWindowTokens === 128_000, "model context window was ignored");
  assert(limits.outputReserveTokens === 8_192, "output reserve is not bounded");
  assert(limits.source === "model", "known model limit lost provenance");
});

check("Auto routing uses the smallest safe candidate window", () => {
  const limits = deriveAiCenterContextLimits([
    { contextWindow: 128_000 },
    { contextWindow: 65_536 },
  ]);
  assert(limits.contextWindowTokens === 65_536, "Auto budget can overflow a candidate");
  const unknown = deriveAiCenterContextLimits([
    { contextWindow: 128_000 },
    {},
  ]);
  assert(unknown.contextWindowTokens === 32_000, "unknown limit did not fail closed");
  assert(unknown.source === "safe-default", "fallback provenance is incorrect");
});

check("short conversations keep every original turn", () => {
  const conversation = conversationWith(["hello", "hi", "what changed?", "nothing yet"]);
  const projection = buildConversationContext(conversation, {
    contextWindowTokens: 8_192,
    outputReserveTokens: 1_024,
  });
  assert(projection.messages.length === 4, "short history was compacted");
  assert(projection.compactedMessageIds.length === 0, "short history has a compaction trace");
  assert(projection.retainedMessageIds.length === 4, "retained history trace is incomplete");
});

check("long conversations are bounded and traceable without mutation", () => {
  const conversation = conversationWith(
    Array.from({ length: 8 }, (_, index) => `${index}:${"上下文".repeat(320)}`),
  );
  const before = JSON.stringify(conversation.messages);
  const projection = buildConversationContext(conversation, {
    contextWindowTokens: 4_096,
    outputReserveTokens: 1_024,
    systemTokens: 128,
  });
  assert(projection.compactedMessageIds.length > 0, "long history was not compacted");
  assert(projection.retainedMessageIds.at(-1) === "message-8", "latest turn was not retained");
  assert(projection.summary?.includes("message-1.."), "compaction range is not traceable");
  assert(
    projection.estimatedInputTokens <= projection.inputBudgetTokens,
    "projected input exceeds its budget",
  );
  assert(JSON.stringify(conversation.messages) === before, "durable history was mutated");
});

check("output reserve and CJK-aware estimation reduce available input", () => {
  const conversation = conversationWith(["a".repeat(2_000)]);
  const smallReserve = buildConversationContext(conversation, {
    contextWindowTokens: 8_192,
    outputReserveTokens: 1_024,
  });
  const largeReserve = buildConversationContext(conversation, {
    contextWindowTokens: 8_192,
    outputReserveTokens: 4_096,
  });
  assert(
    largeReserve.inputBudgetTokens < smallReserve.inputBudgetTokens,
    "output reserve did not reduce input capacity",
  );
  assert(
    estimateContextTokens("上下文上下文") > estimateContextTokens("context"),
    "CJK content is underestimated like ASCII text",
  );
  const withSystemContext = buildConversationContext(conversation, {
    contextWindowTokens: 8_192,
    outputReserveTokens: 1_024,
    systemTokens: 512,
  });
  assert(
    withSystemContext.inputBudgetTokens + 512 + 1_024 + 256 <= 8_192,
    "system context and output reserve exceed the model window",
  );
});

console.log("PASS Beta.4 context budget behavior");
