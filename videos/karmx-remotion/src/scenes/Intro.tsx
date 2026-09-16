import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, colors } from "../components/Layout";
import { LockupMark, SpectrumBars, BassGlow } from "../components/Background";
import { EASE } from "../components/Layout";

export const Intro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const mark = interpolate(frame, [0, 0.7 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const sub = interpolate(frame, [0.3 * fps, 1 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  const pills = ["Context engine", "Predictive compaction", "Local fusion", "Any model"];

  return (
    <Scene center>
      <BassGlow />
      <div
        style={{
          opacity: mark,
          scale: interpolate(mark, [0, 1], [0.94, 1]),
          filter: `blur(${interpolate(mark, [0, 1], [14, 0])}px)`,
        }}
      >
        <LockupMark size={132} />
      </div>
      <p
        style={{
          margin: "18px 0 40px",
          color: colors.dim,
          fontSize: 30,
          fontWeight: 500,
          opacity: sub,
          translate: `0px ${interpolate(sub, [0, 1], [16, 0])}px`,
        }}
      >
        Your terminal coding agent — with new platform features
      </p>
      <div style={{ display: "flex", gap: 14 }}>
        {pills.map((p, i) => {
          const s = 0.5 * fps + i * 0.14 * fps;
          const t = interpolate(frame, [s, s + 0.28 * fps], [0, 1], {
            extrapolateLeft: "clamp",
            extrapolateRight: "clamp",
            easing: Easing.bezier(...EASE),
          });
          return (
            <div
              key={p}
              style={{
                padding: "12px 22px",
                border: `2px solid ${colors.line}`,
                borderRadius: 999,
                background: "rgba(30,41,59,0.88)",
                fontSize: 20,
                fontWeight: 800,
                letterSpacing: "-0.02em",
                opacity: t,
                scale: interpolate(t, [0, 1], [0.9, 1]),
                translate: `0px ${interpolate(t, [0, 1], [14, 0])}px`,
              }}
            >
              {p}
            </div>
          );
        })}
      </div>
      <div style={{ position: "absolute", left: 140, right: 140, bottom: 90 }}>
        <SpectrumBars height={110} />
      </div>
    </Scene>
  );
};
