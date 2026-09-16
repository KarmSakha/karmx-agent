import React from "react";
import { AbsoluteFill, Sequence, useVideoConfig, staticFile, interpolate, Easing } from "remotion";
import { TransitionSeries, linearTiming } from "@remotion/transitions";
import { fade } from "@remotion/transitions/fade";
import { Audio } from "@remotion/media";
import { LightLeak } from "@remotion/light-leaks";
import { EASE } from "./components/Layout";
import { Background } from "./components/Background";
import { Intro } from "./scenes/Intro";
import { Terminal } from "./scenes/Terminal";
import { ContextEngine } from "./scenes/ContextEngine";
import { Compaction } from "./scenes/Compaction";
import { Fusion } from "./scenes/Fusion";
import { Models } from "./scenes/Models";
import { BrowserPreview } from "./scenes/BrowserPreview";
import { Outro } from "./scenes/Outro";

export const KarmxFilm: React.FC = () => {
  const { fps } = useVideoConfig();

  return (
    <AbsoluteFill style={{ background: "#0F172A" }}>
      <Background />
      <Audio
        src={staticFile("audio/bed.mp3")}
        loop
        volume={(f) =>
          interpolate(f, [0, 1.6 * fps, 46 * fps, 50 * fps], [0, 0.5, 0.5, 0], {
            easing: Easing.bezier(...EASE),
            extrapolateLeft: "clamp",
            extrapolateRight: "clamp",
          })
        }
      />
      <Sequence from={27 * fps - 12} durationInFrames={Math.round(1.2 * fps)}>
        <Audio src={staticFile("audio/whoosh.mp3")} volume={0.55} />
      </Sequence>
      <Sequence from={27 * fps + 30} durationInFrames={Math.round(1.5 * fps)}>
        <Audio src={staticFile("audio/impact.mp3")} volume={0.45} />
      </Sequence>
      <Sequence from={47 * fps - 12} durationInFrames={Math.round(1.2 * fps)}>
        <Audio src={staticFile("audio/whoosh.mp3")} volume={0.5} />
      </Sequence>
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={4 * fps}>
          <Intro />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={8 * fps}>
          <Terminal />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={8 * fps}>
          <ContextEngine />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={7 * fps}>
          <Compaction />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={8 * fps}>
          <Fusion />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={6 * fps}>
          <Models />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={6 * fps}>
          <BrowserPreview />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: 8 })}
        />
        <TransitionSeries.Sequence durationInFrames={3 * fps + 56}>
          <Outro />
        </TransitionSeries.Sequence>
      </TransitionSeries>
      <Sequence from={27 * fps} durationInFrames={20}>
        <LightLeak hueShift={80} />
      </Sequence>
      <Sequence from={47 * fps} durationInFrames={20}>
        <LightLeak hueShift={80} />
      </Sequence>
    </AbsoluteFill>
  );
};
