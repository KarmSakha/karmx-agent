import React from "react";
import { loadFont } from "@remotion/google-fonts/Inter";

export const { fontFamily: inter } = loadFont("normal", {
  weights: ["500", "800"],
  subsets: ["latin"],
});

export const colors = {
  bg: "#0F172A",
  panel: "#1E293B",
  bar: "#0B1220",
  hero: "#F8FAFC",
  dim: "#94A3B8",
  accent: "#22C55E",
  accentDark: "#15803D",
  line: "rgba(248,250,252,0.14)",
};

export const mono = 'ui-monospace, "SF Mono", Menlo, monospace';

export const EASE = [0.16, 1, 0.3, 1] as const;

export const Scene: React.FC<{
  children: React.ReactNode;
  center?: boolean;
}> = ({ children, center }) => (
  <div
    style={{
      position: "absolute",
      inset: 0,
      display: "flex",
      flexDirection: "column",
      justifyContent: "center",
      alignItems: center ? "center" : "stretch",
      padding: "140px 140px 120px",
      width: "100%",
      height: "100%",
      fontFamily: inter,
      color: colors.hero,
    }}
  >
    {children}
  </div>
);

export const Kicker: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p
    style={{
      margin: "0 0 14px",
      color: colors.accent,
      fontSize: 20,
      fontWeight: 500,
      letterSpacing: "0.16em",
      textTransform: "uppercase",
    }}
  >
    {children}
  </p>
);

export const Headline: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <h2
    style={{
      margin: "0 0 16px",
      fontSize: 64,
      fontWeight: 800,
      letterSpacing: "-0.04em",
      lineHeight: 1.05,
    }}
  >
    {children}
  </h2>
);

export const Sub: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p
    style={{
      margin: 0,
      color: colors.dim,
      fontSize: 30,
      fontWeight: 500,
      maxWidth: 980,
      lineHeight: 1.35,
    }}
  >
    {children}
  </p>
);
