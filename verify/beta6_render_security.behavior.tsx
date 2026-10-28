import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import katex from "katex";
import MarkdownRenderer from "../src/components/MarkdownRenderer";

function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

const originalTrust = Object.getOwnPropertyDescriptor(Object.prototype, "trust");
try {
  Object.defineProperty(Object.prototype, "trust", { configurable: true, writable: true, value: true });
  for (const expression of [String.raw`\href{javascript:alert(1)}{click}`, String.raw`\includegraphics{https://example.invalid/untrusted-image}`]) {
    const direct = katex.renderToString(expression, { throwOnError: false });
    assert(!/<a\b[^>]*\bhref=["']\s*javascript:/i.test(direct), "inherited trust enabled a script link");
    assert(!/<img\b/i.test(direct), "inherited trust enabled an external image");
    const product = renderToStaticMarkup(createElement(MarkdownRenderer, { content: `$${expression}$` }));
    assert(!/<a\b[^>]*\bhref=["']\s*javascript:/i.test(product), "product renderer accepted a script link");
    assert(!/<img\b/i.test(product), "product renderer loaded an untrusted image");
  }
} finally {
  if (originalTrust) Object.defineProperty(Object.prototype, "trust", originalTrust);
  else Reflect.deleteProperty(Object.prototype, "trust");
}
console.log("✓ inherited prototype trust cannot enable links or images in KaTeX or the product renderer");
const rendered = renderToStaticMarkup(createElement(MarkdownRenderer, { content: "Inline $x^2 + y^2$\n\n$$\\frac{1}{2}$$\n\n<script>alert(1)</script>" }));
assert(rendered.includes('class="katex"') && rendered.includes("mfrac"), "normal inline or block math stopped rendering");
assert(!/<script\b/i.test(rendered), "raw HTML script survived Markdown rendering");
console.log("✓ production Markdown renders inline/block math and excludes raw script elements");
console.log("PASS: Beta6 render security");
