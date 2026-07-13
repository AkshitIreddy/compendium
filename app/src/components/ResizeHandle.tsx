import { useRef } from "react";

/** Draggable vertical divider between panels. Keyboard-adjustable
 * (role=separator + arrow keys), pointer-captured drag, and a commit callback
 * on release so settings persist once per gesture instead of per pixel. */
export function ResizeHandle({
  onDrag,
  onCommit,
  ariaLabel,
}: {
  /** dx in px since the last event (positive = pointer moved right) */
  onDrag: (dx: number) => void;
  onCommit: () => void;
  ariaLabel: string;
}) {
  const lastX = useRef(0);

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      tabIndex={0}
      className="group relative z-10 w-1.5 shrink-0 cursor-col-resize select-none touch-none"
      onPointerDown={(e) => {
        lastX.current = e.clientX;
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        const dx = e.clientX - lastX.current;
        if (dx !== 0) {
          lastX.current = e.clientX;
          onDrag(dx);
        }
      }}
      onPointerUp={(e) => {
        e.currentTarget.releasePointerCapture(e.pointerId);
        onCommit();
      }}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault();
          onDrag(e.key === "ArrowLeft" ? -16 : 16);
          onCommit();
        }
      }}
    >
      {/* visual affordance: hairline that thickens on hover/focus */}
      <div
        className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-edge transition-token
                   group-hover:w-[3px] group-hover:bg-accent group-focus-visible:w-[3px]
                   group-focus-visible:bg-accent"
      />
    </div>
  );
}
