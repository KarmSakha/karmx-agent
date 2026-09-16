import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  staticFile,
} from "remotion";
import { useWindowedAudioData, visualizeAudio } from "@remotion/media-utils";
import { colors, inter } from "./Layout";

export const Background: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = frame / fps;

  const glow = 1 + 0.06 * Math.sin(t * 1.4);
  const drift = interpolate(frame, [0, 50 * fps], [0, 18], {
    extrapolateRight: "clamp",
  });

  return (
    <AbsoluteFill style={{ background: colors.bg }}>
      <div
        style={{
          position: "absolute",
          left: 480,
          top: 100,
          width: 980,
          height: 780,
          borderRadius: "50%",
          background:
            "radial-gradient(circle, rgba(34,197,94,0.18) 0%, rgba(34,197,94,0) 68%)",
          scale: glow,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: -100,
          top: -80,
          width: 680,
          height: 580,
          borderRadius: "50%",
          background:
            "radial-gradient(circle, rgba(90,120,160,0.14) 0%, transparent 70%)",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 960,
          top: 260,
          width: 540,
          height: 540,
          border: "2px solid rgba(34,197,94,0.12)",
          borderRadius: "50%",
          rotate: `${drift}deg`,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: "-8%",
          right: "-8%",
          bottom: "-28%",
          height: "58%",
          backgroundImage:
            "repeating-linear-gradient(90deg, rgba(248,250,252,0.06) 0 1px, transparent 1px 76px), repeating-linear-gradient(0deg, rgba(248,250,252,0.06) 0 1px, transparent 1px 76px)",
          transform: "perspective(900px) rotateX(74deg)",
          transformOrigin: "center top",
          maskImage: "linear-gradient(to bottom, rgba(0,0,0,0.4), transparent 78%)",
        }}
      />
    </AbsoluteFill>
  );
};

export const SpectrumBars: React.FC<{ height?: number }> = ({ height = 140 }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { audioData, dataOffsetInSeconds } = useWindowedAudioData({
    src: staticFile("audio/bed.mp3"),
    frame,
    fps,
    windowInSeconds: 30,
  });

  if (!audioData) return null;

  const bars = visualizeAudio({
    fps,
    frame,
    audioData,
    numberOfSamples: 64,
    optimizeFor: "speed",
    dataOffsetInSeconds,
  });

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-end",
        height,
        gap: 4,
        opacity: 0.5,
      }}
    >
      {bars.map((v, i) => (
        <div
          key={i}
          style={{
            flex: 1,
            height: `${Math.max(2, v * 100)}%`,
            background: `linear-gradient(to top, ${colors.accentDark}, ${colors.accent})`,
            borderRadius: "4px 4px 0 0",
          }}
        />
      ))}
    </div>
  );
};

export const BassGlow: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { audioData, dataOffsetInSeconds } = useWindowedAudioData({
    src: staticFile("audio/bed.mp3"),
    frame,
    fps,
    windowInSeconds: 30,
  });

  if (!audioData) return null;

  const bars = visualizeAudio({
    fps,
    frame,
    audioData,
    numberOfSamples: 8,
    optimizeFor: "speed",
    dataOffsetInSeconds,
  });
  const bass = bars.slice(0, 3).reduce((a, b) => a + b, 0) / 3;
  const s = 1 + bass * 0.22;

  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        top: "44%",
        width: 760,
        height: 760,
        margin: "-380px 0 0 -380px",
        borderRadius: "50%",
        background: `radial-gradient(circle, rgba(34,197,94,${0.16 + bass * 0.18}) 0%, rgba(34,197,94,0) 68%)`,
        scale: s,
        pointerEvents: "none",
      }}
    />
  );
};

export const LockupMark: React.FC<{ size?: number }> = ({ size = 148 }) => (
  <p
    style={{
      margin: 0,
      fontFamily: inter,
      fontSize: size,
      fontWeight: 800,
      letterSpacing: "-0.05em",
      lineHeight: 1,
      color: colors.hero,
    }}
  >
    karm<span style={{ color: colors.accent, textShadow: "0 0 48px rgba(34,197,94,0.5)" }}>X</span>
  </p>
);
