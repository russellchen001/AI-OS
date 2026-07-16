import {
  isValidElement,
  useState,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";

import "katex/dist/katex.min.css";
import "highlight.js/styles/github-dark.css";

type MarkdownRendererProps = {
  content: string;
  fallback?: string;
};

type CodeBlockProps =
  HTMLAttributes<HTMLElement> & {
    inline?: boolean;
    className?: string;
    children?: React.ReactNode;
  };

function extractText(
  node: ReactNode,
): string {
  if (
    typeof node === "string" ||
    typeof node === "number"
  ) {
    return String(node);
  }

  if (Array.isArray(node)) {
    return node
      .map(extractText)
      .join("");
  }

  if (isValidElement<{
    children?: ReactNode;
  }>(node)) {
    return extractText(
      node.props.children,
    );
  }

  return "";
}

function detectCodeLanguage(
  code: string,
): string {
  const value = code.trim();

  if (
    /\b(def|import|from|print|async def|lambda)\b/.test(
      value,
    ) ||
    /__name__\s*==\s*["']__main__["']/.test(
      value,
    )
  ) {
    return "python";
  }

  if (
    /\b(interface|type|enum|namespace)\b/.test(
      value,
    ) ||
    /:\s*(string|number|boolean|unknown|never)\b/.test(
      value,
    )
  ) {
    return "typescript";
  }

  if (
    /\b(const|let|var|function|console\.log)\b/.test(
      value,
    ) ||
    /=>/.test(value)
  ) {
    return "javascript";
  }

  if (
    /\b(fn|let mut|impl|trait|pub struct|println!)\b/.test(
      value,
    )
  ) {
    return "rust";
  }

  if (
    /\b(public|private|protected|class|System\.out)\b/.test(
      value,
    )
  ) {
    return "java";
  }

  if (
    /#include\s*[<"]/.test(value) ||
    /\b(std::|cout\s*<<|cin\s*>>)/.test(
      value,
    )
  ) {
    return "cpp";
  }

  if (
    /\b(package main|func main|fmt\.)\b/.test(
      value,
    )
  ) {
    return "go";
  }

  if (
    /^\s*(SELECT|INSERT|UPDATE|DELETE|CREATE TABLE)\b/im.test(
      value,
    )
  ) {
    return "sql";
  }

  if (
    /^\s*[{[]/.test(value) &&
    /"[^"]+"\s*:/.test(value)
  ) {
    return "json";
  }

  if (
    /<([a-z][\w-]*)(\s|>)/i.test(
      value,
    )
  ) {
    return "html";
  }

  if (
    /(^|\n)\s*[.#]?[\w-]+\s*\{[^}]*\}/s.test(
      value,
    )
  ) {
    return "css";
  }

  if (
    /^\s*(npm|pnpm|yarn|cd|git|cargo|python3?|curl|brew)\b/m.test(
      value,
    )
  ) {
    return "shell";
  }

  return "code";
}

function CodeBlock({
  inline,
  className,
  children,
  ...props
}: CodeBlockProps) {
  const [copied, setCopied] =
    useState(false);

  const code =
    extractText(children)
      .replace(/\n$/, "");

  const declaredLanguage =
    /language-([\w-]+)/.exec(
      className ?? "",
    )?.[1];

  const language =
    declaredLanguage ||
    detectCodeLanguage(code);

  if (inline) {
    return (
      <code
        className={className}
        {...props}
      >
        {children}
      </code>
    );
  }

  const copyCode =
    async () => {
      try {
        await navigator.clipboard.writeText(
          code,
        );

        setCopied(true);

        window.setTimeout(
          () => setCopied(false),
          1600,
        );
      } catch {
        setCopied(false);
      }
    };

  return (
    <div className="markdown-code-block">
      <div className="markdown-code-toolbar">
        <span>
          {language}
        </span>

        <button
          type="button"
          onClick={() => {
            void copyCode();
          }}
        >
          {copied
            ? "✓ Copied"
            : "Copy"}
        </button>
      </div>

      <pre>
        <code
          className={className}
          {...props}
        >
          {children}
        </code>
      </pre>
    </div>
  );
}

export default function MarkdownRenderer({
  content,
  fallback = "",
}: MarkdownRendererProps) {
  const value =
    content.trim().length > 0
      ? content
      : fallback;

  return (
    <div className="markdown-renderer">
      <ReactMarkdown
        remarkPlugins={[
          remarkGfm,
          remarkMath,
        ]}
        rehypePlugins={[
          rehypeKatex,
          rehypeHighlight,
        ]}
        components={{
          code: CodeBlock,
        }}
      >
        {value}
      </ReactMarkdown>
    </div>
  );
}
