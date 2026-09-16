import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE, mono } from "../components/Layout";
import { Card, Ring } from "../components/ui";

export const Fusion: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const left = interpolate(frame, [0, 0.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const right = interpolate(frame, [0.15 * fps, 0.65 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const core = interpolate(frame, [0.6 * fps, 1 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.34, 1.56, 0.64, 1),
  });
  const beam = interpolate(frame, [0.7 * fps, 1 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  const leadP = interpolate(frame, [0.9 * fps, 1.6 * fps], [0, 64], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const sideP = interpolate(frame, [1 * fps, 1.7 * fps], [0, 48], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const agent = (
    title: string,
    tool: string,
    orb: string,
    orbGlow: string,
    chips: string[],
    log: string[],
    ringP: number,
    ringLabel: string,
    t: number,
    x: number,
  ) => (
    <div
      style={{
        opacity: t,
        translate: `${interpolate(t, [0, 1], [x, 0])}px 0px`,
      }}
    >
      <Card style={{ minHeight: 420 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 18 }}>
          <div
            style={{
              width: 20,
              height: 20,
              borderRadius: "50%",
              background: orb,
              boxShadow: `0 0 16px ${orbGlow}`,
            }}
          />
          <p style={{ margin: 0, fontSize: 30, fontWeight: 800, letterSpacing: "-0.03em" }}>{title}</p>
        </div>
        <p style={{ margin: "0 0 14px", color: colors.accent, fontFamily: mono, fontSize: 17, fontWeight: 500 }}>
          {tool}
        </p>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 10, marginBottom: 20 }}>
          {chips.map((c, i) => (
            <span
              key={c}
              style={{
                padding: "8px 14px",
                border: `2px solid ${i < 2 ? "rgba(34,197,94,0.55)" : colors.line}`,
                borderRadius: 999,
                color: i < 2 ? colors.hero : colors.dim,
                background: i < 2 ? "rgba(34,197,94,0.12)" : "transparent",
                fontSize: 17,
                fontWeight: 500,
              }}
            >
              {c}
            </span>
          ))}
        </div>
        <div style={{ fontFamily: mono, fontSize: 19, lineHeight: 1.55 }}>
          {log.map((l, i) => (
            <div key={l} style={{ color: i === 2 ? colors.accent : i === 1 ? colors.dim : colors.hero }}>
              {l}
            </div>
          ))}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 16, marginTop: 28 }}>
          <Ring p={ringP} size={84} />
          <div>
            <div style={{ fontSize: 30, fontWeight: 800 }}>{ringLabel.split("|")[0]}</div>
            <div style={{ color: colors.dim, fontSize: 17, fontWeight: 500 }}>{ringLabel.split("|")[1]}</div>
          </div>
        </div>
      </Card>
    </div>
  );

  return (
    <Scene>
      <Kicker>Two models · one identity</Kicker>
      <Headline>Local fusion</Headline>
      <Sub>
        A persistent lead + sidekick split. Different models, different context windows, independent compaction. The
        user still sees one agent.
      </Sub>
      <div style={{ position: "relative", marginTop: 26 }}>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 22 }}>
          {agent(
            "Lead · claude",
            "sidekick tool · owner",
            "#D97757",
            "rgba(217,119,87,0.7)",
            ["user-facing", "judgement", "authority actions"],
            ["review the diff", "own the conversation", "delegate mechanical work"],
            leadP,
            "own context|compaction 70 / 78 / 100",
            left,
            -40,
          )}
          {agent(
            "Sidekick · custom",
            "sidekick · persistent",
            colors.accent,
            "rgba(34,197,94,0.7)",
            ["persistent", "own window", "not a subagent"],
            ["search + trace", "implement the patch", "survives across turns"],
            sideP,
            "own model|KARMX_SIDEKICK_MODEL",
            right,
            40,
          )}
        </div>
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: "50%",
            width: 96,
            height: 96,
            margin: "-48px 0 0 -48px",
            borderRadius: "50%",
            border: `2px solid rgba(34,197,94,0.7)`,
            background: "radial-gradient(circle at 40% 35%, rgba(34,197,94,0.45), #1E293B 72%)",
            boxShadow: "0 0 44px rgba(34,197,94,0.35)",
            opacity: core,
            scale: interpolate(core, [0, 1], [0.6, 1]),
          }}
        />
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: "50%",
            width: 150,
            height: 6,
            marginTop: -3,
            marginLeft: -225,
            borderRadius: 99,
            background: colors.accent,
            boxShadow: "0 0 18px rgba(34,197,94,0.85)",
            opacity: beam,
            scale: `${beam} 1`,
            transformOrigin: "right center",
          }}
        />
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: "50%",
            width: 150,
            height: 6,
            marginTop: -3,
            marginLeft: 75,
            borderRadius: 99,
            background: colors.accent,
            boxShadow: "0 0 18px rgba(34,197,94,0.85)",
            opacity: beam,
            scale: `${beam} 1`,
            transformOrigin: "left center",
          }}
        />
      </div>
    </Scene>
  );
};
