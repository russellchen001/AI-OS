import { useEffect, useRef, useState, type FormEvent } from "react";
import { listMemory, saveMemory } from "../services/memory";
import {
  completeChatTaskExecution,
  executeChatWorkTask,
  failChatTaskExecution,
  startChatTaskExecution,
  submitChatTask,
  type ChatTaskType,
} from "../services/tasks";
import {
  listAiCenterModels,
  streamThroughAiCenter,
  type AiCenterStream,
  type AiCenterModelChoice,
} from "../services/aiCenter";
import MarkdownRenderer from "../components/MarkdownRenderer";
import { PROVIDERS_CHANGED_EVENT } from "../services/providers";
import {
  attachmentsFromFiles,
  buildConversationContext,
  getConversation,
  saveConversation,
  type ConversationAttachment,
  type ConversationMessage,
} from "../services/conversations";
import {
  applyOutboundLanguage,
  detectCurrentLanguage,
  detectMemoryLanguage,
  type ResponseLanguage,
} from "./chatLanguagePolicy";

type ChatMessage = ConversationMessage;

function invocationCost(message: ChatMessage): string {
  const metadata = message.invocation;
  if (!metadata) return "";
  if (!metadata.pricingMatched || metadata.estimatedCostUsd === undefined) {
    return "Cost unavailable";
  }
  if (metadata.estimatedCostUsd === 0) return "$0.00 estimated";
  return `$${metadata.estimatedCostUsd.toFixed(4)} estimated`;
}

type ChatPageProps = {
  conversationId: string;
  onOpenMyAi: () => void;
  onAddAgent: () => void;
};


function extractMemoryCandidate(content: string): string | undefined {
  const text = content.trim();
  const prefixes = ["请记住", "帮我记住", "记住"];
  const prefix = prefixes.find((candidate) => text.startsWith(candidate));

  if (!prefix) return undefined;

  const memory = text
    .slice(prefix.length)
    .replace(/^[：:，,。.\s]+/, "")
    .trim();

  return memory || undefined;
}

async function buildMemoryContext(): Promise<
  | {
      message: { role: "system"; content: string };
      language: ResponseLanguage | undefined;
    }
  | undefined
> {
  const memories = (await listMemory())
    .filter((entry) => entry.type === "user")
    .map((entry) => entry.content.trim())
    .filter(Boolean);

  if (memories.length === 0) return undefined;
  const language = detectMemoryLanguage(memories);
  const languagePolicy = language
    ? `Default response language is ${language}. Answer in ${language} unless the CURRENT user message explicitly asks for another language.`
    : undefined;

  return {
    language,
    message: {
      role: "system",
      content: [
        "Long-term user memory applies as the user's default preferences unless the CURRENT user message explicitly overrides it.",
        "A temporary instruction from an earlier user message applies only to that earlier response and must not carry forward.",
        "Do not infer the preferred language for the current response from the language used by previous assistant messages.",
        ...(languagePolicy ? ["", "Runtime language policy:", languagePolicy] : []),
        "Do not mention or repeat these memories unless the user asks.",
        "",
        "Long-term user memory:",
        ...memories.map((memory) => `- ${memory}`),
      ].join("\n"),
    },
  };
}

function buildLanguageOverride(
  content: string,
): { role: "system"; content: string } | undefined {
  const language = detectCurrentLanguage(content);
  if (!language) return undefined;

  return {
    role: "system",
    content: `Current-request language override: Answer this request in ${language}. This instruction takes priority over long-term user memory and applies only to this response.`,
  };
}

function ChatPage({ conversationId, onOpenMyAi, onAddAgent }: ChatPageProps) {
  const [draft, setDraft] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const [selectedModel, setSelectedModel] = useState<AiCenterModelChoice>();
  const [availableModels, setAvailableModels] = useState(() => listAiCenterModels());
  const [taskType, setTaskType] = useState<ChatTaskType>("ASK");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activeStream, setActiveStream] = useState<AiCenterStream>();
  const [attachments, setAttachments] = useState<ConversationAttachment[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setMessages(getConversation(conversationId)?.messages ?? []);
    setAttachments([]);
  }, [conversationId]);

  useEffect(() => {
    const refreshModels = () => {
      const models = listAiCenterModels();
      setAvailableModels(models);
      setSelectedModel((current) =>
        current &&
        models.some(
          (model) =>
            model.providerInstanceId === current.providerInstanceId &&
            model.modelId === current.modelId,
        )
          ? current
          : undefined,
      );
    };
    window.addEventListener(PROVIDERS_CHANGED_EVENT, refreshModels);
    window.addEventListener("storage", refreshModels);
    return () => {
      window.removeEventListener(PROVIDERS_CHANGED_EVENT, refreshModels);
      window.removeEventListener("storage", refreshModels);
    };
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if (!content || isSubmitting) return;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content,
      createdAt: new Date().toISOString(),
      attachments,
    };
    const nextMessages = [...messages, userMessage];

    const conversation = getConversation(conversationId);
    if (!conversation) return;

    const memoryCandidate = extractMemoryCandidate(content);
    if (memoryCandidate) {
      setIsSubmitting(true);
      try {
        const timestamp = new Date().toISOString();
        await saveMemory({
          id: crypto.randomUUID(),
          type: "user",
          content: memoryCandidate,
          metadata: {
            source: "chat",
            importance: 5,
          },
          createdAt: timestamp,
          updatedAt: timestamp,
        });

        const localMessages: ChatMessage[] = [
          ...nextMessages,
          {
            id: crypto.randomUUID(),
            role: "assistant",
            content: "好的，我记住了。",
            createdAt: new Date().toISOString(),
          },
        ];
        saveConversation({
          ...conversation,
          title:
            conversation.messages.length === 0
              ? content.slice(0, 48)
              : conversation.title,
          messages: localMessages,
        });
        setMessages(localMessages);
        setAttachments([]);
        setDraft("");
      } finally {
        setIsSubmitting(false);
      }
      return;
    }

    const nextConversation = {
      ...conversation,
      title:
        conversation.messages.length === 0
          ? content.slice(0, 48)
          : conversation.title,
      messages: nextMessages,
    };
    const context = buildConversationContext(nextConversation);
    if (context.summary && context.summary !== conversation.summary) {
      nextConversation.summary = context.summary;
    }
    saveConversation(nextConversation);
    setMessages(nextMessages);
    setAttachments([]);
    const memoryContext = await buildMemoryContext();
    const languageOverride = buildLanguageOverride(content);
    const responseLanguage =
      detectCurrentLanguage(content) ?? memoryContext?.language;
    const currentMessage = context.messages[context.messages.length - 1];
    const priorMessages = context.messages.slice(0, -1);
    const outboundCurrentMessage = currentMessage
      ? {
          ...currentMessage,
          content: applyOutboundLanguage(currentMessage.content, responseLanguage),
        }
      : undefined;
    const conversationMessages = [
      ...(memoryContext ? [memoryContext.message] : []),
      ...(languageOverride ? [languageOverride] : []),
      ...priorMessages,
      ...(outboundCurrentMessage ? [outboundCurrentMessage] : []),
    ];
    setDraft("");
    setIsSubmitting(true);

    let activeTaskId: string | undefined;
    let activeAssistantMessageId: string | undefined;
    let activeAssistantText = "";
    try {
      const task = await submitChatTask(content, taskType);
      activeTaskId = task.taskId;
      if (task.status === "READY") {
        await startChatTaskExecution(task.taskId);
        const assistantMessageId = crypto.randomUUID();
        activeAssistantMessageId = assistantMessageId;
        setMessages((current) => [
          ...current,
          {
            id: assistantMessageId,
            role: "assistant",
            content: "",
            createdAt: new Date().toISOString(),
          },
        ]);
        const stream = streamThroughAiCenter(
          conversationMessages,
          selectedModel,
          (chunk) => {
            activeAssistantText += chunk;
            setMessages((current) =>
              current.map((message) =>
                message.id === assistantMessageId
                  ? { ...message, content: message.content + chunk }
                  : message,
                ),
            );
          },
        );
        setActiveStream(stream);
        const { response: answer, cancelled } = await stream.result;
        setActiveStream(undefined);
        if (cancelled) {
          await failChatTaskExecution(task.taskId, "Cancelled by user");
          setMessages((current) =>
            current.map((message) =>
              message.id === assistantMessageId
                ? {
                    ...message,
                    content:
                      message.content || "Response stopped before any text was received.",
                    invocation: answer.metadata,
                  }
                : message,
            ),
          );
          const stoppedMessages = getConversation(conversationId)?.messages ?? nextMessages;
          saveConversation({
            ...nextConversation,
            messages: [
              ...stoppedMessages.filter((message) => message.id !== assistantMessageId),
              {
                id: assistantMessageId,
                role: "assistant",
                content: answer.text || "Response stopped before any text was received.",
                createdAt: new Date().toISOString(),
                invocation: answer.metadata,
              },
            ],
          });
          return;
        }
        await completeChatTaskExecution(task.taskId, {
          providerId: answer.providerId,
          modelId: answer.modelId,
          text: answer.text,
        });
        setMessages((current) =>
          current.map((message) =>
            message.id === assistantMessageId
              ? { ...message, invocation: answer.metadata }
              : message,
          ),
        );
        saveConversation({
          ...nextConversation,
          messages: [
            ...nextMessages,
            {
              id: assistantMessageId,
              role: "assistant",
              content: answer.text,
              createdAt: new Date().toISOString(),
              invocation: answer.metadata,
            },
          ],
        });
        return;
      }
      const execution = await executeChatWorkTask(task.taskId, "openclaw");
      const workMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: execution.output
          ? `OpenClaw completed the plan.\n\n\`\`\`json\n${JSON.stringify(execution.output, null, 2)}\n\`\`\``
          : `OpenClaw completed plan ${execution.planId}.`,
        createdAt: new Date().toISOString(),
      };
      const completedMessages = [...nextMessages, workMessage];
      setMessages(completedMessages);
      saveConversation({
        ...nextConversation,
        messages: completedMessages,
      });
    } catch (error) {
      if (activeTaskId) {
        await failChatTaskExecution(activeTaskId).catch(() => undefined);
      }
      const needsProvider =
        error instanceof Error && error.message === "NO_CONNECTED_PROVIDER";
      const failureMessage = needsProvider
        ? "Connect and test an AI in My AI before starting a conversation."
        : "AI‑OS could not complete this request. Check the selected AI connection and try again.";
      setMessages((current) =>
        activeAssistantMessageId
          ? current.map((message) =>
              message.id === activeAssistantMessageId
                ? {
                    ...message,
                    content: message.content
                      ? `${message.content}\n\nResponse interrupted.`
                      : failureMessage,
                  }
                : message,
            )
          : [
              ...current,
              {
                id: crypto.randomUUID(),
                role: "assistant",
                content: failureMessage,
                createdAt: new Date().toISOString(),
              },
            ],
      );
      const persisted = getConversation(conversationId);
      if (persisted && activeAssistantMessageId) {
        saveConversation({
          ...persisted,
          messages: [
            ...nextMessages,
            {
              id: activeAssistantMessageId,
              role: "assistant",
              content: activeAssistantText
                ? `${activeAssistantText}\n\nResponse interrupted.`
                : failureMessage,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
    } finally {
      setActiveStream(undefined);
      setIsSubmitting(false);
    }
  }

  return (
    <section className="chat-workspace" aria-label="Conversation">
      <header className="chat-topbar">
        <button type="button" className="conversation-title">
          <span>{getConversation(conversationId)?.title ?? "New conversation"}</span>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 8 4 4 4-4" /></svg>
        </button>
        <button type="button" className="quiet-icon-button" aria-label="Share conversation">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12m0-12 4 4m-4-4L8 7M5 13v6h14v-6" /></svg>
        </button>
      </header>

      <div className={messages.length ? "chat-thread" : "chat-empty"}>
        {messages.length ? messages.map((message) => (
          <article key={message.id} className={`chat-message chat-message-${message.role}`}>
            <span className="message-author">{message.role === "user" ? "You" : "AI‑OS"}</span>
            {message.role === "assistant" ? (
              <>
                <MarkdownRenderer
                  content={message.content}
                  fallback={isSubmitting ? "Thinking…" : ""}
                  artifactSource="Chat"
                />
                {message.invocation && (
                  <div className="message-invocation" aria-label="AI response details">
                    <span>{message.invocation.source === "local" ? "On this Mac" : "Cloud"}</span>
                    <span>{message.invocation.providerId} · {message.invocation.modelId}</span>
                    <span>{message.invocation.latencyMs.toLocaleString()} ms</span>
                    <span>{invocationCost(message)}</span>
                    {message.invocation.fallbackOccurred && (
                      <span>{message.invocation.attempts.length} attempts</span>
                    )}
                  </div>
                )}
                {message.content && (
                  <button
                    type="button"
                    className="message-copy-button"
                    onClick={() => void navigator.clipboard.writeText(message.content)}
                  >
                    Copy
                  </button>
                )}
              </>
            ) : (
              <p>{message.content}</p>
            )}
          </article>
        )) : (
          <div className="chat-welcome">
            <div className="ai-orbit" aria-hidden="true"><span /><span /><span /></div>
            <p className="chat-eyebrow">AI‑OS</p>
            <h1>What would you like to get done?</h1>
            <p className="chat-intro">
              Ask a question, work with a file, or describe an outcome. AI‑OS will choose the
              right model and tools for the job.
            </p>
            <div className="starter-grid">
              <button type="button" onClick={() => setDraft("Summarize the key points in this file")}>
                <span>Work with a file</span><small>Summarize, compare, or extract</small>
              </button>
              <button type="button" onClick={() => setDraft("Help me plan this task step by step")}>
                <span>Plan something</span><small>Turn an outcome into clear steps</small>
              </button>
              <button type="button" onClick={onOpenMyAi}>
                <span>Choose an AI</span><small>Connect an account or local model</small>
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="composer-dock">
        <form className="chat-composer" onSubmit={submit}>
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
              }
            }}
            placeholder="Message AI‑OS"
            rows={1}
            aria-label="Message AI-OS"
          />
          <div className="composer-rail">
            <div className="composer-tools">
              <button type="button" className="composer-icon-button" aria-label="Attach files" onClick={() => fileInputRef.current?.click()}>
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
              </button>
              <input
                ref={fileInputRef}
                className="visually-hidden"
                type="file"
                multiple
                onChange={(event) => {
                  if (!event.target.files) return;
                  void attachmentsFromFiles(event.target.files).then(setAttachments);
                  event.target.value = "";
                }}
              />
              <div className="model-picker">
                <button
                  type="button"
                  className="model-pill"
                  aria-expanded={modelMenuOpen}
                  onClick={() => setModelMenuOpen((current) => !current)}
                >
                  <span className="model-status" />
                  {selectedModel?.label ?? "Auto"}
                  <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 8 4 4 4-4" /></svg>
                </button>
                {modelMenuOpen && (
                  <div className="model-menu" role="menu">
                    <button
                      type="button"
                      className={!selectedModel ? "model-menu-active" : ""}
                      onClick={() => {
                        setSelectedModel(undefined);
                        setModelMenuOpen(false);
                      }}
                    >
                      <strong>Auto</strong>
                      <small>Use the default connected AI</small>
                    </button>
                    {availableModels.map((model) => (
                      <button
                        type="button"
                        key={`${model.providerInstanceId}:${model.modelId}`}
                        className={
                          selectedModel?.providerInstanceId === model.providerInstanceId &&
                          selectedModel.modelId === model.modelId
                            ? "model-menu-active"
                            : ""
                        }
                        onClick={() => {
                          setSelectedModel(model);
                          setModelMenuOpen(false);
                        }}
                      >
                        <strong>{model.label}</strong>
                      </button>
                    ))}
                    {!availableModels.length && (
                      <button type="button" onClick={onOpenMyAi}>
                        <strong>Connect an AI</strong>
                        <small>Open My AI to add a Provider</small>
                      </button>
                    )}
                  </div>
                )}
              </div>
              <button
                type="button"
                className="task-mode-pill"
                onClick={() => setTaskType((current) => current === "ASK" ? "DO" : "ASK")}
                aria-label={`Task mode: ${taskType === "ASK" ? "Chat" : "Work"}`}
              >
                {taskType === "ASK" ? "Chat" : "Work"}
              </button>
              <button type="button" className="tool-pill">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v4m0 10v4M3 12h4m10 0h4M5.6 5.6l2.8 2.8m7.2 7.2 2.8 2.8m0-12.8-2.8 2.8m-7.2 7.2-2.8 2.8" /></svg>
                Tools
              </button>
              <div className="agent-picker">
                <button
                  type="button"
                  className="agent-pill"
                  aria-expanded={agentMenuOpen}
                  onClick={() => setAgentMenuOpen((current) => !current)}
                >
                  <span className="agent-glyph">O</span>
                  OpenClaw
                  <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 8 4 4 4-4" /></svg>
                </button>
                {agentMenuOpen && (
                  <div className="agent-menu" role="menu">
                    <p>Run this task with</p>
                    <button type="button" className="agent-menu-item agent-menu-active" role="menuitem" onClick={() => setAgentMenuOpen(false)}>
                      <span className="agent-menu-mark">O</span>
                      <span><strong>OpenClaw</strong><small>Default · Ready</small></span>
                      <span className="agent-check">✓</span>
                    </button>
                    <button type="button" className="agent-menu-item" role="menuitem" onClick={onAddAgent}>
                      <span className="agent-menu-mark agent-menu-mark-muted">H</span>
                      <span><strong>Hermes Agent</strong><small>Not added</small></span>
                    </button>
                    <button type="button" className="add-agent-item" role="menuitem" onClick={onAddAgent}>
                      <span>+</span> Add an agent
                    </button>
                  </div>
                )}
              </div>
            </div>
            {activeStream ? (
              <button
                type="button"
                className="send-button stop-button"
                aria-label="Stop response"
                onClick={() => void activeStream.cancel()}
              >
                <span />
              </button>
            ) : (
              <button type="submit" className="send-button" disabled={!draft.trim() || isSubmitting} aria-label="Send">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 7-7 7 7M12 5v14" /></svg>
              </button>
            )}
          </div>
        </form>
        {attachments.length > 0 && (
          <div className="composer-attachments">
            {attachments.map((attachment) => (
              <span key={attachment.id}>
                {attachment.name}
                <button
                  type="button"
                  aria-label={`Remove ${attachment.name}`}
                  onClick={() =>
                    setAttachments((current) =>
                      current.filter((item) => item.id !== attachment.id),
                    )
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        <p>AI‑OS can make mistakes. Review important actions before approving them.</p>
      </div>
    </section>
  );
}

export default ChatPage;
