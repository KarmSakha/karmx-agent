import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE, mono } from "../components/Layout";
import { Card, CardTitle } from "../components/ui";

export const Compaction: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // tokens ramp to spawn (70%), then apply (78%), then summary applies and drops
  const tokens = interpolate(
    frame,
    [0.2 * fps, 1.8 * fps, 2.7 * fps, 3.6 * fps],
    [0, 183500, 204000, 84000],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.bezier(...EASE),
    }
  );
  const fill = Math.min(tokens / 262144, 1);

  const msgs = ["add stripe checkout", "codebase_retrieval 48 spans", "patch cart.ts + tests", "browser_preview 24 DOM"];
  const msgFade = interpolate(frame, [3 * fps, 3.5 * fps], [1, 0.22], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const summaryIn = interpolate(frame, [1.9 * fps, 2.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const chipIn = interpolate(frame, [2.6 * fps, 2.9 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  return (
    <Scene>
      <Kicker>Background, not a stall</Kicker>
      <Headline>Predictive compaction</Headline>
      <Sub>
        Summarize while there is still headroom. Apply the ready summary before the window fills. Never wait on the
        hard limit.
      </Sub>

      <div style={{ marginTop: 26 }}>
        <div style={{ fontSize: 64, fontWeight: 800, letterSpacing: "-0.04em" }}>
          {Math.round(tokens).toLocaleString("en-US")}{" "}
          <span style={{ color: colors.dim, fontSize: 30, fontWeight: 500 }}>/ 262,144 tokens</span>
        </div>
        <div style={{ position: "relative", height: 26, marginTop: 14, color: colors.dim, fontSize: 17, fontWeight: 800 }}>
          <span style={{ position: "absolute", left: "0%" }}>idle</span>
          <span style={{ position: "absolute", left: "70%", marginLeft: -28 }}>spawn</span>
          <span style={{ position: "absolute", left: "78%", marginLeft: -24 }}>apply</span>
          <span style={{ position: "absolute", right: "0%" }}>hard</span>
        </div>
        <div
          style={{
            height: 30,
            borderRadius: 99,
            background: "rgba(248,250,252,0.08)",
            overflow: "hidden",
            marginTop: 8,
          }}
        >
          <div
            style={{
              width: `${fill * 100}%`,
              height: "100%",
              background: "linear-gradient(90deg, #15803d, #22c55e)",
              borderRadius: 99,
            }}
          />
        </div>
        <div style={{ position: "relative", height: 16, marginTop: 4 }}>
          {[0, 70, 78, 99].map((p) => (
            <div
              key={p}
              style={{
                position: "absolute",
                left: `${p}%`,
                top: 0,
                width: 2,
                height: 14,
                background: "rgba(248,250,252,0.35)",
              }}
            />
          ))}
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 22, marginTop: 18 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {msgs.map((m, i) => {
            const s = 0.4 * fps + i * 0.18 * fps;
            const t = interpolate(frame, [s, s + 0.25 * fps], [0, 1], {
              extrapolateLeft: "clamp",
              extrapolateRight: "clamp",
              easing: Easing.bezier(...EASE),
            });
            return (
              <div
                key={m}
                style={{
                  padding: "14px 16px",
                  border: `2px solid ${colors.line}`,
                  borderRadius: 12,
                  background: "rgba(30,41,59,0.95)",
                  fontSize: 20,
                  fontWeight: 500,
                  opacity: t * msgFade,
                  translate: `0px ${interpolate(t, [0, 1], [16, 0])}px`,
                }}
              >
                <em
                  style={{
                    color: colors.accent,
                    fontStyle: "normal",
                    fontFamily: mono,
                    fontSize: 15,
                    fontWeight: 800,
                    letterSpacing: "0.08em",
                    textTransform: "uppercase",
                    marginRight: 8,
                  }}
                >
                  {i === 0 ? "user" : i === 2 ? "asst" : "tool"}
                </em>
                {m}
              </div>
            );
          })}
        </div>
        <div
          style={{
            opacity: summaryIn,
            scale: interpolate(summaryIn, [0, 1], [0.96, 1]),
            translate: `0px ${interpolate(summaryIn, [0, 1], [20, 0])}px`,
          }}
        >
          <Card
            style={{
              minHeight: 300,
              border: `2px solid rgba(34,197,94,0.5)`,
              background: "rgba(34,197,94,0.08)",
              display: "flex",
              flexDirection: "column",
              justifyContent: "center",
            }}
          >
            <CardTitle>Ready summary</CardTitle>
            <p style={{ margin: 0, color: colors.dim, fontSize: 22, fontWeight: 500, lineHeight: 1.4 }}>
              Checkout work, cart rounding, preview connected. Continue from the patch — no stall on the user’s turn.
            </p>
            <div
              style={{
                marginTop: 22,
                alignSelf: "flex-start",
                padding: "8px 16px",
                borderRadius: 999,
                background: colors.accent,
                color: "#06210d",
                fontSize: 18,
                fontWeight: 800,
                opacity: chipIn,
                scale: interpolate(chipIn, [0, 1], [0.9, 1]),
              }}
            >
              apply · swap in
            </div>
          </Card>
        </div>
      </div>
    </Scene>
  );
};
