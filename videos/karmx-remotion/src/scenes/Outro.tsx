import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, colors, EASE } from "../components/Layout";
import { LockupMark, BassGlow } from "../components/Background";

export const Outro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const mark = interpolate(frame, [0, 0.7 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const sub = interpolate(frame, [0.3 * fps, 0.9 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });

  return (
    <Scene center>
      <BassGlow />
      <div
        style={{
          opacity: mark,
          scale: interpolate(mark, [0, 1], [0.94, 1]),
          filter: `blur(${interpolate(mark, [0, 1], [12, 0])}px)`,
        }}
      >
        <LockupMark size={148} />
      </div>
      <p
        style={{
          margin: "18px 0 0",
          color: colors.dim,
          fontSize: 32,
          fontWeight: 500,
          opacity: sub,
          translate: `0px ${interpolate(sub, [0, 1], [14, 0])}px`,
        }}
      >
        Context. Compaction. Fusion. Any model.
      </p>
    </Scene>
  );
};
