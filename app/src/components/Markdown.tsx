import { memo, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSanitize from "rehype-sanitize";
import { CodeBlock } from "./CodeBlock";

/** Sanitized GFM markdown with Shiki code blocks — the one markdown renderer
 * used everywhere (advisories, technique cards, notebook markdown cells). */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="prose-compendium">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSanitize]}
        components={{
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className ?? "");
            const text = String(children).replace(/\n$/, "");
            if (match && text.includes("\n")) {
              return <CodeBlock code={text} lang={match[1]} />;
            }
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          },
          pre({ children }) {
            // CodeBlock renders its own <pre> via Shiki; unwrap ours when the
            // child is a fenced block, keep it for indented code.
            const child = Array.isArray(children) ? children[0] : children;
            if (
              child &&
              typeof child === "object" &&
              "props" in (child as { props?: { className?: string } }) &&
              /language-/.test((child as { props: { className?: string } }).props.className ?? "")
            ) {
              return <>{children}</>;
            }
            return <pre>{children}</pre>;
          },
          a({ href, children }) {
            return (
              <a href={href} target="_blank" rel="noreferrer noopener">
                {children as ReactNode}
              </a>
            );
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
