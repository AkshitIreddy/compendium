// README demo GIF — gifsmith scene.
// Storyboard: establishing shot of the app as a window on a Windows desktop,
// cursor clicks a use-case example prompt, the pipeline stages animate, the
// advisory renders; scroll the recommendations, open a citation's source,
// read it, close, return to the empty state (seamless anchor loop).
//
// Run (dev server + demo fixture required):
//   cd app && npm run dev        # terminal 1
//   npx gifsmith render demo.config.mjs   # terminal 2
import { timeline, web } from "gifsmith";
import { cursor, taskbar } from "gifsmith/props";

const tl = timeline((t) => {
  t.waitFor("main");
  t.hold(1.8); // establishing shot: desktop, taskbar, the app window
  t.loopAnchor();

  // click the first example prompt (the legal-PDF use case) — sends immediately
  t.click("div.grid.w-full.max-w-xl > button:first-child", { via: "cursor" });

  // pipeline progress stages animate (~6s in demo mode), then the advisory
  t.waitFor('[data-turn="2"]', { timeout: 15000 });
  t.hold(1.6);

  // read the dossier + recommendation cards
  t.scroll('[role="log"]', 520, 3.2);
  t.hold(1.2);

  // open a cited source: the technique card, then its source notebook
  t.click("button.citation-mark", { via: "cursor" });
  t.waitFor('aside[aria-label="Source panel"]');
  t.hold(1.4);
  t.scroll('aside[aria-label="Source panel"] .overflow-y-auto', 380, 2.4);
  t.hold(0.8);
  t.click("aside button.w-fit", { via: "cursor" }); // Open source notebook →
  t.waitFor("[data-cell]");
  t.hold(1.0);
  t.scroll('aside[aria-label="Source panel"] .overflow-y-auto', 420, 2.6);
  t.hold(0.8);
  t.click('button[aria-label="Close source panel"]', { via: "cursor" });
  t.hold(0.6);

  // back to the neutral state so the loop closes seamlessly
  t.click('nav[aria-label="Conversation history"] button.flex-1', { via: "cursor" });
  t.hold(1.4);
});

export default {
  target: web("http://localhost:1420/?demo=1"),
  out: "../assets/demo.gif",
  alsoEmit: ["webp"],
  compose: "stage",
  stage: { title: "Compendium", os: "windows" },
  props: [taskbar({ os: "windows" }), cursor()],
  timeline: tl,
  encode: { width: 880, fps: 14, speed: 1.3, colors: 96, targetMB: 4 },
};
