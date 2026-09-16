import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE, mono } from "../components/Layout";
import { Card, CardTitle } from "../components/ui";

const Dot: React.FC<{ c: string }> = ({ c }) => (
  <div style={{ width: 12, height: 12, borderRadius: "50%", background: c }} />
);

const TypeLine: React.FC<{
  frame: number;
  fps: number;
  start: number;
  text: string;
  prompt?: string;
  cps?: number;
}> = ({ frame, fps, start, text, prompt = "$", cps = 22 }) => {
  const chars = Math.floor(
    interpolate(frame, [start, start + (text.length / cps) * fps], [0, text.length], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    }),
  );
  const shown = text.slice(0, chars);
  const typing = frame >= start && chars < text.length;
  const caretOn = typing && Math.floor(frame / (fps * 0.28)) % 2 === 0;
  if (frame < start) {
    return null;
  }
  return (
    <div style={{ whiteSpace: "nowrap", fontFamily: mono }}>
      <span style={{ color: colors.accent, fontWeight: 800, marginRight: 12 }}>{prompt}</span>
      <span style={{ color: colors.hero }}>{shown}</span>
      {caretOn ? (
        <span
          style={{
            display: "inline-block",
            width: 11,
            height: 24,
            marginLeft: 3,
            background: colors.accent,
            verticalAlign: -4,
          }}
        />
      ) : null}
    </div>
  );
};

const OutLine: React.FC<{
  frame: number;
  fps: number;
  start: number;
  children: React.ReactNode;
  tone?: "dim" | "ok" | "tool";
}> = ({ frame, fps, start, children, tone = "dim" }) => {
  const t = interpolate(frame, [start, start + 0.18 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  if (t <= 0) {
    return null;
  }
  return (
    <div
      style={{
        fontFamily: mono,
        opacity: t,
        translate: `${interpolate(t, [0, 1], [10, 0])}px 0px`,
        color: tone === "ok" ? colors.accent : tone === "tool" ? colors.hero : colors.dim,
        fontWeight: tone === "ok" ? 800 : 500,
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </div>
  );
};

const TaskChip: React.FC<{
  frame: number;
  fps: number;
  start: number;
  label: string;
  ok?: boolean;
}> = ({ frame, fps, start, label, ok }) => {
  const t = interpolate(frame, [start, start + 0.22 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  if (t <= 0) {
    return null;
  }
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "12px 14px",
        border: `2px solid ${ok ? "rgba(34,197,94,0.45)" : colors.line}`,
        borderRadius: 14,
        background: ok ? "rgba(34,197,94,0.08)" : "rgba(248,250,252,0.03)",
        opacity: t,
        translate: `0px ${interpolate(t, [0, 1], [14, 0])}px`,
        scale: interpolate(t, [0, 1], [0.96, 1]),
      }}
    >
      <div
        style={{
          width: 18,
          height: 18,
          borderRadius: "50%",
          background: ok ? colors.accent : colors.line,
          flex: "0 0 auto",
        }}
      />
      <span style={{ fontFamily: mono, fontSize: 18, fontWeight: 500 }}>{label}</span>
    </div>
  );
};

export const Terminal: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const win = interpolate(frame, [0, 0.4 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const taskIn = interpolate(frame, [0.18 * fps, 0.58 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const progress = interpolate(frame, [1.4 * fps, 6.4 * fps], [8, 100], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const done = interpolate(frame, [6.5 * fps, 6.9 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  return (
    <Scene>
      <Kicker>Typed in the shell</Kicker>
      <Headline>
        Just type <span style={{ color: colors.accent }}>@karmx</span>
      </Headline>
      <Sub>Run a task, preview it, then hand the mechanical work to fusion — without leaving the terminal.</Sub>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1.15fr 0.85fr",
          gap: 22,
          marginTop: 26,
        }}
      >
        <div
          style={{
            opacity: win,
            translate: `${interpolate(win, [0, 1], [-22, 0])}px 0px`,
            scale: interpolate(win, [0, 1], [0.98, 1]),
          }}
        >
          <Card style={{ padding: 0, overflow: "hidden", minHeight: 520 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                height: 46,
                padding: "0 18px",
                borderBottom: `2px solid ${colors.line}`,
                background: colors.bar,
              }}
            >
              <Dot c="#FF5F57" />
              <Dot c="#FEBC2E" />
              <Dot c="#28C840" />
              <div style={{ flex: 1, textAlign: "center", color: colors.dim, fontSize: 18, fontWeight: 500 }}>
                karmx — zsh
              </div>
            </div>
            <div
              style={{
                padding: "24px 28px",
                fontFamily: mono,
                fontSize: 22,
                lineHeight: 1.55,
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              <TypeLine frame={frame} fps={fps} start={0.28 * fps} text="karmx run" />
              <OutLine frame={frame} fps={fps} start={0.82 * fps}>
                session karmx · lead claude · sidekick custom
              </OutLine>
              <OutLine frame={frame} fps={fps} start={1.02 * fps}>
                aliases @karmx @kx
              </OutLine>

              <TypeLine frame={frame} fps={fps} start={1.28 * fps} text="@karmx fix the checkout total" />
              <OutLine frame={frame} fps={fps} start={2.7 * fps} tone="tool">
                ● codebase_retrieval 48 spans
              </OutLine>
              <OutLine frame={frame} fps={fps} start={3.0 * fps} tone="ok">
                ✓ enhance_prompt ready
              </OutLine>

              <TypeLine frame={frame} fps={fps} start={3.4 * fps} text="/preview" />
              <OutLine frame={frame} fps={fps} start={4.0 * fps} tone="ok">
                ✓ browser_preview localhost:5173
              </OutLine>

              <TypeLine frame={frame} fps={fps} start={4.5 * fps} text="/fusion" />
              <OutLine frame={frame} fps={fps} start={5.1 * fps}>
                lead claude · sidekick custom
              </OutLine>
              <OutLine frame={frame} fps={fps} start={5.45 * fps} tone="ok">
                ✓ sidekick patching cart.ts
              </OutLine>
            </div>
          </Card>
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 18,
            opacity: taskIn,
            translate: `${interpolate(taskIn, [0, 1], [22, 0])}px 0px`,
          }}
        >
          <Card style={{ minHeight: 250 }}>
            <CardTitle>Task</CardTitle>
            <p style={{ margin: "0 0 18px", color: colors.dim, fontSize: 22, fontWeight: 500 }}>
              Fix the checkout total
            </p>
            <div style={{ height: 10, borderRadius: 99, background: "rgba(248,250,252,0.1)", overflow: "hidden" }}>
              <div
                style={{
                  width: `${progress}%`,
                  height: "100%",
                  background: colors.accent,
                  borderRadius: 99,
                }}
              />
            </div>
            <p style={{ margin: "12px 0 0", fontFamily: mono, fontSize: 18, color: colors.accent, fontWeight: 800 }}>
              {Math.round(progress)}%
            </p>
          </Card>
          <Card>
            <CardTitle>Running</CardTitle>
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              <TaskChip frame={frame} fps={fps} start={2.7 * fps} label="codebase_retrieval" ok={frame > 2.9 * fps} />
              <TaskChip frame={frame} fps={fps} start={4.0 * fps} label="browser_preview" ok={frame > 4.2 * fps} />
              <TaskChip frame={frame} fps={fps} start={5.45 * fps} label="fusion · cart.ts" ok={frame > 5.7 * fps} />
            </div>
            <div
              style={{
                marginTop: 16,
                padding: "12px 14px",
                borderRadius: 14,
                background: "rgba(34,197,94,0.12)",
                color: colors.accent,
                fontWeight: 800,
                fontSize: 20,
                opacity: done,
                scale: interpolate(done, [0, 1], [0.96, 1]),
              }}
            >
              total $42.00 · patched
            </div>
          </Card>
        </div>
      </div>
    </Scene>
  );
};
