import React from "react";
import { interpolate, Easing } from "remotion";
import { colors, inter, mono, EASE } from "./Layout";

export const enterStyle = (
  frame: number,
  start: number,
  dur = 24,
): React.CSSProperties => {
  const t = interpolate(frame, [start, start + dur], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  return {
    opacity: t,
    translate: `0px ${interpolate(t, [0, 1], [28, 0])}px`,
    scale: interpolate(t, [0, 1], [0.97, 1]),
  };
};

export const Card: React.FC<{
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ children, style }) => (
  <div
    style={{
      padding: "26px 28px 28px",
      border: `2px solid ${colors.line}`,
      borderRadius: 20,
      background: "rgba(30,41,59,0.94)",
      boxShadow: "0 32px 70px rgba(0,0,0,0.42)",
      ...style,
    }}
  >
    {children}
  </div>
);

export const CardTitle: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p style={{ margin: "0 0 6px", fontSize: 30, fontWeight: 800, letterSpacing: "-0.03em", fontFamily: inter }}>
    {children}
  </p>
);

export const CardTool: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p style={{ margin: "0 0 18px", color: colors.accent, fontFamily: mono, fontSize: 17, fontWeight: 500 }}>
    {children}
  </p>
);

export const Hit: React.FC<{
  name: string;
  score: string;
  width: number;
  dimmed?: boolean;
  frame: number;
  start: number;
}> = ({ name, score, width, dimmed, frame, start }) => {
  const bar = interpolate(frame, [start, start + 20], [0, width], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        marginBottom: 14,
        padding: "12px 14px",
        border: `2px solid ${colors.line}`,
        borderRadius: 12,
        background: "rgba(248,250,252,0.03)",
        opacity: dimmed ? 0.4 : 1,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 20, fontWeight: 500 }}>
        <span style={{ fontFamily: mono }}>{name}</span>
        <span style={{ color: dimmed ? colors.dim : colors.accent, fontWeight: 800 }}>{score}</span>
      </div>
      <div style={{ height: 8, borderRadius: 99, background: "rgba(248,250,252,0.1)", overflow: "hidden" }}>
        <div style={{ width: `${bar}%`, height: "100%", background: colors.accent, borderRadius: 99 }} />
      </div>
    </div>
  );
};

export const Ring: React.FC<{ p: number; size?: number }> = ({ p, size = 80 }) => (
  <div
    style={{
      width: size,
      height: size,
      borderRadius: "50%",
      background: `conic-gradient(${colors.accent} ${p}%, rgba(248,250,252,0.1) 0)`,
      flex: "0 0 auto",
    }}
  >
    <div
      style={{
        width: size - 24,
        height: size - 24,
        margin: 12,
        borderRadius: "50%",
        background: colors.panel,
      }}
    />
  </div>
);
