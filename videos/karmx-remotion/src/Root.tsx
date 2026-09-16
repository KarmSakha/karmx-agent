import React from "react";
import { Composition } from "remotion";
import { KarmxFilm } from "./KarmxFilm";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="karmx"
        component={KarmxFilm}
        durationInFrames={50 * 30}
        fps={30}
        width={1920}
        height={1080}
      />
    </>
  );
};
