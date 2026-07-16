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
      >
        {value}
      </ReactMarkdown>
    </div>
  );
}
