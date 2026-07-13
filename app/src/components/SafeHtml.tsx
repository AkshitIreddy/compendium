import { memo, useMemo } from "react";
import DOMPurify from "dompurify";

/** Sanitized HTML rendering for pack-provided strings (attribution notices).
 * Packs are semi-trusted (a user could import a third-party .pack someday),
 * so anything HTML-shaped from a manifest goes through DOMPurify with a
 * links-and-emphasis-only allowlist. */
export const SafeHtml = memo(function SafeHtml({
  html,
  className,
}: {
  html: string;
  className?: string;
}) {
  const clean = useMemo(
    () =>
      DOMPurify.sanitize(html, {
        ALLOWED_TAGS: ["a", "strong", "em", "b", "i", "code", "br", "span"],
        ALLOWED_ATTR: ["href", "title"],
        ALLOWED_URI_REGEXP: /^https?:/i,
      }),
    [html],
  );
  return <span className={className} dangerouslySetInnerHTML={{ __html: clean }} />;
});
