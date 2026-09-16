import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE, mono } from "../components/Layout";

export const Models: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const picker = interpolate(frame, [0, 0.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  const leadSel = interpolate(frame, [1.1 * fps, 1.4 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const tabSwap = interpolate(frame, [1.9 * fps, 2.2 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const sideSel = interpolate(frame, [2.3 * fps, 2.6 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const leadRows = [
    ["Claude", "Anthropic · judgement"],
    ["GPT-4.1", "OpenAI"],
    ["Ollama", "local · llama"],
    ["Custom", "your OpenAI-compatible URL"],
  ];
  const sideRows = [
    ["Same as lead", "inherit"],
    ["Custom", "KARMX_SIDEKICK · your endpoint"],
    ["OpenAI", "gpt-4o-mini"],
    ["Ollama", "any local model you run"],
  ];

  const col = (title: string, rows: string[][], active: boolean, selIdx: number, selT: number) => (
    <div>
      <div
        style={{
          padding: "18px 24px",
          borderBottom: `2px solid ${colors.line}`,
          background: active ? colors.accent : colors.bar,
          color: active ? "#06210d" : colors.hero,
          fontSize: 22,
          fontWeight: 800,
        }}
      >
        {title}
      </div>
      {rows.map((r, i) => {
        const on = i === selIdx && selT > 0.5;
        return (
          <div
            key={r[0]}
            style={{
              padding: "18px 24px",
              borderBottom: `2px solid ${colors.line}`,
              background: on ? "rgba(34,197,94,0.12)" : "transparent",
            }}
          >
            <div style={{ fontSize: 24, fontWeight: 800, letterSpacing: "-0.03em" }}>{r[0]}</div>
            <div style={{ color: colors.dim, fontSize: 17, fontWeight: 500 }}>{r[1]}</div>
          </div>
        );
      })}
    </div>
  );

  return (
    <Scene>
      <Kicker>Lead and sidekick independently</Kicker>
      <Headline>Pick any model</Headline>
      <Sub>
        Anthropic, OpenAI, or a custom OpenAI-compatible endpoint you configure. Tab assigns the sidekick a different model
        from the lead.
      </Sub>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 0,
          marginTop: 30,
          border: `2px solid ${colors.line}`,
          borderRadius: 20,
          overflow: "hidden",
          background: "rgba(30,41,59,0.96)",
          opacity: picker,
          translate: `0px ${interpolate(picker, [0, 1], [24, 0])}px`,
        }}
      >
        {col("Lead", leadRows, tabSwap < 0.5, leadSel < 0.5 ? 0 : 3, leadSel)}
        {col("Sidekick", sideRows, tabSwap >= 0.5, sideSel < 0.5 ? 0 : 1, sideSel)}
      </div>
      <p style={{ marginTop: 18, color: colors.dim, fontFamily: mono, fontSize: 17, fontWeight: 500 }}>
        ↑↓ select · tab sidekick · enter confirm · @karmx / @kx
      </p>
    </Scene>
  );
};
