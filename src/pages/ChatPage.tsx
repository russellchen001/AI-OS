import { useEffect, useRef, useState, type FormEvent } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { listMemory, saveMemory, type MemoryEntry } from "../services/memory";
import {
  completeChatTaskExecution,
  executeChatWorkTask,
  failChatTaskExecution,
  startChatTaskExecution,
  submitChatTask,
  describeChatTaskError,
  type ChatTaskType,
} from "../services/tasks";
import {
  listAiCenterModels,
  streamThroughAiCenter,
  type AiCenterStream,
  type AiCenterModelChoice,
} from "../services/aiCenter";
import MarkdownRenderer from "../components/MarkdownRenderer";
import { useDialog } from "../components/DialogProvider";
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
  applyMemoryPolicyToOutbound,
  describeMemoryPolicy,
  resolveMemoryPolicy,
  type MemoryPolicy,
} from "../services/memoryPolicy";

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

function formatMessageTime(createdAt: string | undefined): string {
  if (!createdAt) return "";
  const timestamp = new Date(createdAt);
  if (Number.isNaN(timestamp.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(timestamp);
}

function formatFilesystemScanResult(output: unknown): string {
  if (!output || typeof output !== "object") {
    throw new Error("OpenClaw returned an invalid folder scan result.");
  }
  const entries = (output as { entries?: unknown }).entries;
  if (!Array.isArray(entries) || entries.some((entry) => typeof entry !== "string")) {
    throw new Error("OpenClaw returned an invalid folder scan result.");
  }
  return entries.length
    ? `Folder scan completed.\n\n${entries.map((entry) => `- ${entry}`).join("\n")}`
    : "Folder scan completed. The folder is empty.";
}

function formatFilesystemReadResult(output: unknown): string {
  if (!output || typeof output !== "object") {
    throw new Error("OpenClaw returned an invalid file read result.");
  }
  const result = output as {
    content?: unknown;
    limitBytes?: unknown;
    mimeType?: unknown;
    size?: unknown;
    status?: unknown;
    truncated?: unknown;
  };
  if (typeof result.mimeType !== "string" || typeof result.size !== "number") {
    throw new Error("OpenClaw returned an invalid file read result.");
  }
  if (result.status === "unsupported") {
    return `This file cannot be displayed as text.\n\nType: ${result.mimeType}\nSize: ${result.size.toLocaleString()} bytes`;
  }
  if (result.status === "too_large" && typeof result.limitBytes === "number") {
    return `This file is too large to read safely.\n\nSize: ${result.size.toLocaleString()} bytes\nLimit: ${result.limitBytes.toLocaleString()} bytes`;
  }
  if (result.status !== "text" || typeof result.content !== "string") {
    throw new Error("OpenClaw returned an invalid file read result.");
  }
  const longestFence = Math.max(
    2,
    ...Array.from(result.content.matchAll(/`+/g), (match) => match[0].length),
  );
  const fence = "`".repeat(longestFence + 1);
  const truncated = result.truncated === true
    ? "\n\nContent was truncated to the safe output limit."
    : "";
  return `File read completed.\n\n${fence}\n${result.content}\n${fence}${truncated}`;
}

function formatFilesystemWriteResult(output: unknown): string {
  if (!output || typeof output !== "object") {
    throw new Error("OpenClaw returned an invalid file write result.");
  }
  const result = output as {
    bytesWritten?: unknown;
    path?: unknown;
    status?: unknown;
  };
  if (typeof result.path !== "string") {
    throw new Error("OpenClaw returned an invalid file write result.");
  }
  if (result.status === "exists") {
    return `The file was not written because a file already exists at the selected path.\n\n${result.path}`;
  }
  if (result.status === "failed") {
    return `OpenClaw could not create the file. No existing file was overwritten.\n\n${result.path}`;
  }
  if (result.status !== "written" || typeof result.bytesWritten !== "number") {
    throw new Error("OpenClaw returned an invalid file write result.");
  }
  return `File created.\n\nPath: ${result.path}\nBytes written: ${result.bytesWritten.toLocaleString()}`;
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

function buildMemoryContext(
  memories: MemoryEntry[],
  policy: MemoryPolicy,
): { role: "system"; content: string } | undefined {
  const policyLines = describeMemoryPolicy(policy);
  if (memories.length === 0 && policyLines.length === 0) return undefined;
  return {
    role: "system",
    content: [
      "Long-term user memory applies as default context unless the CURRENT user message explicitly overrides it.",
      "Current-request overrides apply only to this response and must not carry forward.",
      ...(policyLines.length
        ? ["", "Resolved runtime policy:", ...policyLines]
        : []),
      ...(memories.length
        ? [
            "",
            "Long-term user memory:",
            ...memories.map((entry) => `- ${entry.content.trim()}`),
          ]
        : []),
      "Do not mention or repeat these memories unless the user asks.",
    ].join("\n"),
  };
}

function ChatPage({ conversationId, onOpenMyAi, onAddAgent }: ChatPageProps) {
  const dialog = useDialog();
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
  const [scanFolderPath, setScanFolderPath] = useState<string>();
  const [readFilePath, setReadFilePath] = useState<string>();
  const [writeFilePath, setWriteFilePath] = useState<string>();
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setMessages(getConversation(conversationId)?.messages ?? []);
    setAttachments([]);
    setScanFolderPath(undefined);
    setReadFilePath(undefined);
    setWriteFilePath(undefined);
  }, [conversationId]);

  async function chooseFolderToScan() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a folder to scan",
    });
    if (typeof selected !== "string") return;
    setScanFolderPath(selected);
    setReadFilePath(undefined);
    setWriteFilePath(undefined);
    setTaskType("DO");
    setDraft((current) => current || "Scan this folder");
  }

  async function chooseFileToRead() {
    const selected = await openDialog({
      directory: false,
      multiple: false,
      title: "Choose a text file to read",
    });
    if (typeof selected !== "string") return;
    setReadFilePath(selected);
    setScanFolderPath(undefined);
    setWriteFilePath(undefined);
    setTaskType("DO");
    setDraft((current) => current || "Read this file");
  }

  async function chooseFileToWrite() {
    const selected = await saveDialog({
      title: "Choose where to create the text file",
      defaultPath: "untitled.txt",
    });
    if (typeof selected !== "string") return;
    setWriteFilePath(selected);
    setScanFolderPath(undefined);
    setReadFilePath(undefined);
    setTaskType("DO");
  }

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
    const isWorkRequest = Boolean(scanFolderPath || readFilePath || writeFilePath) || taskType === "DO";

    if (scanFolderPath) {
      const confirmed = await dialog.confirm({
        title: "Scan this folder?",
        message: `OpenClaw will read the folder contents at:\n\n${scanFolderPath}`,
        confirmLabel: "Scan folder",
        cancelLabel: "Cancel",
        tone: "warning",
      });
      if (!confirmed) return;
    }
    if (readFilePath) {
      const confirmed = await dialog.confirm({
        title: "Read this file?",
        message: `OpenClaw will read file contents at:\n\n${readFilePath}`,
        confirmLabel: "Read file",
        cancelLabel: "Cancel",
        tone: "warning",
      });
      if (!confirmed) return;
    }
    if (writeFilePath) {
      const byteLength = new TextEncoder().encode(draft).byteLength;
      const confirmed = await dialog.confirm({
        title: "Create this text file?",
        message: `OpenClaw will create a new file at:\n\n${writeFilePath}\n\nContent size: ${byteLength.toLocaleString()} bytes\nExisting files will not be overwritten.`,
        confirmLabel: "Create file",
        cancelLabel: "Cancel",
        tone: "warning",
      });
      if (!confirmed) return;
    }

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
    const userMemories = (await listMemory()).filter(
      (entry) => entry.type === "user" && entry.content.trim(),
    );
    const { resolvedPolicy } = resolveMemoryPolicy(userMemories, content);
    const memoryContext = buildMemoryContext(userMemories, resolvedPolicy);
    const currentMessage = context.messages[context.messages.length - 1];
    const priorMessages = context.messages.slice(0, -1);
    const outboundCurrentMessage = currentMessage
      ? applyMemoryPolicyToOutbound(currentMessage, resolvedPolicy)
      : undefined;
    const conversationMessages = [
      ...(memoryContext ? [memoryContext] : []),
      ...priorMessages,
      ...(outboundCurrentMessage ? [outboundCurrentMessage] : []),
    ];
    setDraft("");
    setIsSubmitting(true);

    let activeTaskId: string | undefined;
    let activeAssistantMessageId: string | undefined;
    let activeAssistantText = "";
    try {
      const task = await submitChatTask(content, isWorkRequest ? "DO" : "ASK");
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
      const execution = await executeChatWorkTask(
        task.taskId,
        "openclaw",
        writeFilePath
          ? {
              capability: "filesystem.write",
              input: { path: writeFilePath, content: draft, overwrite: false },
              userConfirmed: true,
            }
          : readFilePath
          ? {
              capability: "filesystem.read",
              input: { path: readFilePath },
              userConfirmed: true,
            }
          : scanFolderPath
          ? {
              capability: "filesystem.scan",
              input: { path: scanFolderPath },
              userConfirmed: true,
            }
          : undefined,
      );
      const workMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: writeFilePath
          ? formatFilesystemWriteResult(execution.output)
          : readFilePath
          ? formatFilesystemReadResult(execution.output)
          : scanFolderPath
            ? formatFilesystemScanResult(execution.output)
          : execution.output
            ? `OpenClaw completed the plan.\n\n\`\`\`json\n${JSON.stringify(execution.output, null, 2)}\n\`\`\``
            : `OpenClaw completed plan ${execution.planId}.`,
        createdAt: new Date().toISOString(),
      };
      const completedMessages = [...nextMessages, workMessage];
      setMessages(completedMessages);
      setScanFolderPath(undefined);
      setReadFilePath(undefined);
      setWriteFilePath(undefined);
      saveConversation({
        ...nextConversation,
        messages: completedMessages,
      });
    } catch (error) {
      const workFailureMessage = isWorkRequest
        ? describeChatTaskError(
            error,
            true,
            writeFilePath ? "file write" : readFilePath ? "file read" : "folder scan",
          )
        : undefined;
      if (activeTaskId) {
        await failChatTaskExecution(
          activeTaskId,
          workFailureMessage ?? "AI Center request failed",
        ).catch(() => undefined);
      }
      const failureMessage = describeChatTaskError(
        error,
        isWorkRequest,
        writeFilePath ? "file write" : readFilePath ? "file read" : "folder scan",
      );
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
            {formatMessageTime(message.createdAt) && (
              <time className="message-author" dateTime={message.createdAt}>
                {` · ${formatMessageTime(message.createdAt)}`}
              </time>
            )}
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
                onClick={() =>
                  setTaskType((current) => {
                    const next = current === "ASK" ? "DO" : "ASK";
                    if (next === "ASK") {
                      setScanFolderPath(undefined);
                      setReadFilePath(undefined);
                      setWriteFilePath(undefined);
                    }
                    return next;
                  })
                }
                aria-label={`Task mode: ${taskType === "ASK" ? "Chat" : "Work"}`}
              >
                {taskType === "ASK" ? "Chat" : "Work"}
              </button>
              <button
                type="button"
                className={scanFolderPath ? "tool-pill tool-pill-active" : "tool-pill"}
                aria-label="Choose a folder to scan"
                onClick={() => void chooseFolderToScan()}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2 2h9v9.5H3zM3 7.5V5h7l2 2" /></svg>
                Scan folder
              </button>
              <button
                type="button"
                className={readFilePath ? "tool-pill tool-pill-active" : "tool-pill"}
                aria-label="Choose a file to read"
                onClick={() => void chooseFileToRead()}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h8l4 4v14H6zM14 3v5h5M9 12h6M9 16h6" /></svg>
                Read file
              </button>
              <button
                type="button"
                className={writeFilePath ? "tool-pill tool-pill-active" : "tool-pill"}
                aria-label="Choose where to create a text file"
                onClick={() => void chooseFileToWrite()}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h9l3 3v15H6zM9 13h6M12 10v6" /></svg>
                Write file
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
        {scanFolderPath && (
          <div className="composer-scan-target" role="status">
            <span>
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2 2h9v9.5H3zM3 7.5V5h7l2 2" /></svg>
              <span><strong>Folder scan</strong><small>{scanFolderPath}</small></span>
            </span>
            <button
              type="button"
              aria-label="Cancel folder scan"
              onClick={() => setScanFolderPath(undefined)}
            >
              ×
            </button>
          </div>
        )}
        {readFilePath && (
          <div className="composer-scan-target" role="status">
            <span>
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h8l4 4v14H6zM14 3v5h5M9 12h6M9 16h6" /></svg>
              <span><strong>File read</strong><small>{readFilePath}</small></span>
            </span>
            <button
              type="button"
              aria-label="Cancel file read"
              onClick={() => setReadFilePath(undefined)}
            >
              ×
            </button>
          </div>
        )}
        {writeFilePath && (
          <div className="composer-scan-target" role="status">
            <span>
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h9l3 3v15H6zM9 13h6M12 10v6" /></svg>
              <span><strong>File write</strong><small>{writeFilePath}</small></span>
            </span>
            <button
              type="button"
              aria-label="Cancel file write"
              onClick={() => setWriteFilePath(undefined)}
            >
              ×
            </button>
          </div>
        )}
        <p>AI‑OS can make mistakes. Review important actions before approving them.</p>
      </div>
    </section>
  );
}

export default ChatPage;
