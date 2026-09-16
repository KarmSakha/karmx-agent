import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate, Easing } from "remotion";
import { Scene, Kicker, Headline, Sub, colors, EASE, mono } from "../components/Layout";
import { Card, CardTitle } from "../components/ui";

const Dot: React.FC<{ c: string }> = ({ c }) => (
  <div style={{ width: 12, height: 12, borderRadius: "50%", background: c }} />
);

const ProductCard: React.FC<{
  frame: number;
  fps: number;
  start: number;
  swatch: string;
  name: string;
  meta: string;
  price: string;
}> = ({ frame, fps, start, swatch, name, meta, price }) => {
  const t = interpolate(frame, [start, start + 0.32 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 16,
        padding: "16px 18px",
        border: `2px solid ${colors.line}`,
        borderRadius: 16,
        background: "rgba(248,250,252,0.04)",
        opacity: t,
        translate: `0px ${interpolate(t, [0, 1], [22, 0])}px`,
        scale: interpolate(t, [0, 1], [0.96, 1]),
      }}
    >
      <div
        style={{
          width: 58,
          height: 58,
          borderRadius: 14,
          background: swatch,
          boxShadow: "inset 0 0 0 2px rgba(248,250,252,0.12)",
          flex: "0 0 auto",
        }}
      />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 24, fontWeight: 800, letterSpacing: "-0.03em" }}>{name}</div>
        <div style={{ color: colors.dim, fontSize: 18, fontWeight: 500 }}>{meta}</div>
      </div>
      <div style={{ fontSize: 24, fontWeight: 800 }}>{price}</div>
    </div>
  );
};

export const BrowserPreview: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const win = interpolate(frame, [0, 0.42 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const side = interpolate(frame, [0.16 * fps, 0.56 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const total = interpolate(frame, [1.45 * fps, 2.25 * fps], [0, 42], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const totalCard = interpolate(frame, [1.25 * fps, 1.55 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const pulse = interpolate(frame, [2.3 * fps, 2.7 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const pay = interpolate(frame, [2.55 * fps, 2.95 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(...EASE),
  });
  const scan = interpolate(frame, [0.7 * fps, 3.4 * fps], [8, 86], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const logs = [
    ["[bridge]", "page connected"],
    ["[dom]", "24 nodes captured"],
    ["[console]", "0 errors"],
    ["[perf]", "12ms snapshot"],
  ];

  return (
    <Scene>
      <Kicker>Rendered DOM + console</Kicker>
      <Headline>Browser preview</Headline>
      <Sub>In-process page bridge. Live checkout cards, the real tree, and a 12ms snapshot — not a screenshot guess.</Sub>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1.2fr 0.9fr",
          gap: 22,
          marginTop: 24,
        }}
      >
        <div
          style={{
            opacity: win,
            translate: `${interpolate(win, [0, 1], [-20, 0])}px 0px`,
            scale: interpolate(win, [0, 1], [0.98, 1]),
          }}
        >
          <Card style={{ padding: 0, overflow: "hidden", minHeight: 540, position: "relative" }}>
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
                localhost:5173 · checkout
              </div>
            </div>
            <div
              style={{
                position: "absolute",
                left: 18,
                right: 18,
                top: `${scan}%`,
                height: 2,
                background: "linear-gradient(90deg, transparent, rgba(34,197,94,0.7), transparent)",
                opacity: 0.7,
                pointerEvents: "none",
              }}
            />
            <div style={{ padding: "22px 22px 24px", display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ fontSize: 22, fontWeight: 800, letterSpacing: "-0.03em", marginBottom: 4 }}>Cart</div>
              <ProductCard
                frame={frame}
                fps={fps}
                start={0.35 * fps}
                swatch="#334155"
                name="Studio jacket"
                meta="qty 1 · slate"
                price="$28"
              />
              <ProductCard
                frame={frame}
                fps={fps}
                start={0.55 * fps}
                swatch="#1E3A2F"
                name="Tee"
                meta="qty 1 · forest"
                price="$14"
              />
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "18px 20px",
                  border: `2px solid ${pulse > 0.4 ? "rgba(34,197,94,0.7)" : colors.line}`,
                  borderRadius: 16,
                  background: pulse > 0.4 ? "rgba(34,197,94,0.1)" : "rgba(248,250,252,0.04)",
                  boxShadow: pulse > 0.4 ? "0 0 28px rgba(34,197,94,0.18)" : "none",
                  opacity: totalCard,
                  translate: `0px ${interpolate(totalCard, [0, 1], [16, 0])}px`,
                  scale: interpolate(totalCard, [0, 1], [0.97, 1]),
                }}
              >
                <div>
                  <div style={{ color: colors.dim, fontSize: 18, fontWeight: 500 }}>Total</div>
                  <div style={{ fontSize: 36, fontWeight: 800, letterSpacing: "-0.04em" }}>
                    ${total.toFixed(2)}
                  </div>
                </div>
                <div
                  style={{
                    fontFamily: mono,
                    fontSize: 16,
                    color: pulse > 0.4 ? colors.accent : colors.dim,
                    fontWeight: 800,
                  }}
                >
                  {pulse > 0.4 ? "fixed" : "was $0.00"}
                </div>
              </div>
              <div
                style={{
                  marginTop: 4,
                  height: 52,
                  borderRadius: 14,
                  background: colors.accent,
                  color: "#052E16",
                  fontSize: 22,
                  fontWeight: 800,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  opacity: pay,
                  scale: interpolate(pay, [0, 1], [0.94, 1]),
                }}
              >
                Pay $42.00
              </div>
            </div>
          </Card>
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 18,
            opacity: side,
            translate: `${interpolate(side, [0, 1], [20, 0])}px 0px`,
          }}
        >
          <Card style={{ minHeight: 236 }}>
            <CardTitle>Console</CardTitle>
            <div style={{ fontFamily: mono, fontSize: 19, lineHeight: 1.65 }}>
              {logs.map(([tag, msg], i) => {
                const s = 0.7 * fps + i * 0.2 * fps;
                const t = interpolate(frame, [s, s + 0.18 * fps], [0, 1], {
                  extrapolateLeft: "clamp",
                  extrapolateRight: "clamp",
                });
                return (
                  <div key={msg} style={{ opacity: t, color: colors.dim }}>
                    <span style={{ color: colors.accent, fontWeight: 800 }}>{tag}</span> {msg}
                  </div>
                );
              })}
            </div>
          </Card>
          <Card style={{ minHeight: 286 }}>
            <CardTitle>DOM tree</CardTitle>
            <div style={{ fontFamily: mono, fontSize: 18, lineHeight: 1.7, color: colors.dim }}>
              <div>&lt;html&gt;</div>
              <div style={{ paddingLeft: 22 }}>
                &lt;main&gt; <span style={{ color: colors.accent }}>24 nodes</span>
              </div>
              <div style={{ paddingLeft: 44 }}>&lt;section class="cart"&gt;</div>
              <div
                style={{
                  paddingLeft: 66,
                  marginTop: 6,
                  padding: "8px 12px",
                  borderRadius: 10,
                  background: pulse > 0.4 ? "rgba(34,197,94,0.12)" : "transparent",
                  color: pulse > 0.4 ? colors.accent : colors.dim,
                  fontWeight: 800,
                }}
              >
                &lt;total&gt; ${total.toFixed(2)}
              </div>
            </div>
          </Card>
        </div>
      </div>
    </Scene>
  );
};
