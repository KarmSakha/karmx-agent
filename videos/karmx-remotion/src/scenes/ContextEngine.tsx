import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE } from "../components/Layout";
import { Card, CardTitle, CardTool, Hit, Ring } from "../components/ui";

export const ContextEngine: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const cardIn = (i: number) =>
    interpolate(frame, [i * 0.18 * fps, i * 0.18 * fps + 0.5 * fps], [0, 1], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.bezier(...EASE),
    });

  const toIn = interpolate(frame, [1.1 * fps, 1.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  const budget = interpolate(frame, [0.4 * fps, 1.1 * fps], [0, 72], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  return (
    <Scene>
      <Kicker>In-process · not MCP</Kicker>
      <Headline>Context engine</Headline>
      <Sub>Budgeted workspace search, relevance ranking, and prompt rewrite — on the session’s own model.</Sub>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 22, marginTop: 34 }}>
        <div style={{ opacity: cardIn(0), translate: `0px ${interpolate(cardIn(0), [0, 1], [26, 0])}px` }}>
          <Card>
            <CardTool>codebase_retrieval</CardTool>
            <CardTitle>Find the files</CardTitle>
            <Hit name="src/app.tsx" score="0.94" width={94} frame={frame} start={0.25 * fps} />
            <Hit name="preview.html" score="0.81" width={81} frame={frame} start={0.4 * fps} />
            <Hit name="cart.ts" score="0.67" width={67} frame={frame} start={0.55 * fps} />
            <div style={{ display: "flex", alignItems: "center", gap: 16, marginTop: 20 }}>
              <Ring p={budget} size={84} />
              <div>
                <div style={{ fontSize: 32, fontWeight: 800 }}>20k</div>
                <div style={{ color: colors.dim, fontSize: 18, fontWeight: 500 }}>character budget</div>
              </div>
            </div>
          </Card>
        </div>
        <div style={{ opacity: cardIn(1), translate: `0px ${interpolate(cardIn(1), [0, 1], [26, 0])}px` }}>
          <Card>
            <CardTool>rerank_context</CardTool>
            <CardTitle>Keep what matters</CardTitle>
            <Hit name="checkout total" score="1" width={96} frame={frame} start={0.7 * fps} />
            <Hit name="cart reducer" score="2" width={74} frame={frame} start={0.85 * fps} />
            <Hit name="unrelated css" score="drop" width={18} dimmed frame={frame} start={1 * fps} />
          </Card>
        </div>
        <div style={{ opacity: cardIn(2), translate: `0px ${interpolate(cardIn(2), [0, 1], [26, 0])}px` }}>
          <Card>
            <CardTool>enhance_prompt</CardTool>
            <CardTitle>Make the ask specific</CardTitle>
            <p
              style={{
                margin: 0,
                padding: "14px 16px",
                borderRadius: 12,
                color: colors.dim,
                background: "rgba(248,250,252,0.04)",
                fontSize: 22,
                fontWeight: 500,
              }}
            >
              “fix the cart”
            </p>
            <p style={{ margin: "14px 0", color: colors.accent, fontSize: 20, fontWeight: 800 }}>→ enhanced</p>
            <p
              style={{
                margin: 0,
                padding: "14px 16px",
                borderRadius: 12,
                border: `2px solid rgba(34,197,94,0.45)`,
                background: "rgba(34,197,94,0.08)",
                fontSize: 22,
                fontWeight: 500,
                lineHeight: 1.35,
                opacity: toIn,
                translate: `0px ${interpolate(toIn, [0, 1], [14, 0])}px`,
              }}
            >
              Investigate checkout rounding in cart.ts against the preview DOM, then patch the total.
            </p>
          </Card>
        </div>
      </div>
    </Scene>
  );
};
