import { memo, useEffect, useState } from "react";
import { highlight } from "../lib/highlight";

/** Syntax-highlighted code block with graceful plain-text fallback while
 * Shiki loads (or for unknown languages). */
export const CodeBlock = memo(function CodeBlock({
  code,
  lang,
}: {
  code: string;
  lang: string;
}) {
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    highlight(code, lang)
      .then((h) => !cancelled && setHtml(h))
      .catch(() => !cancelled && setHtml(null));
    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  if (html) {
    // Shiki output is generated from code text (never raw HTML pass-through).
    return <div className="shiki-block" dangerouslySetInnerHTML={{ __html: html }} />;
  }
  return (
    <pre>
      <code>{code}</code>
    </pre>
  );
});
