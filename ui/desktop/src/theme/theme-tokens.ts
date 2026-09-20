/**
 * Theme tokens — the single source of truth for all MCP semantic token values.
 *
 * Every key in McpUiStyleVariableKey must be present in both lightTokens and
 * darkTokens. The TypeScript compiler enforces this: if the SDK adds a new key,
 * the build breaks until both maps are updated.
 *
 * Values are applied to :root via style.setProperty() before first paint
 * (see renderer.tsx). main.css only registers the variable names for Tailwind
 * class generation — it does NOT define values.
 *
 * These tokens serve two purposes:
 *  1. Goose desktop — applied to :root per resolved theme.
 *  2. MCP apps — encoded as light-dark() in hostContext.styles.variables.
 */
import type {
  McpUiHostStyles,
  McpUiStyleVariableKey,
  McpUiStyles,
} from '@modelcontextprotocol/ext-apps/app-bridge';

type ThemeTokens = Record<McpUiStyleVariableKey, string>;

// Subset of keys that are the same across both themes.
type BaseTokenKey = Extract<
  McpUiStyleVariableKey,
  `--font-${string}` | `--border-radius-${string}` | `--border-width-${string}`
>;

type ColorTokenKey = Exclude<McpUiStyleVariableKey, BaseTokenKey>;

// ---------------------------------------------------------------------------
// Base tokens — shared across light and dark themes
// ---------------------------------------------------------------------------
const baseTokens: Pick<ThemeTokens, BaseTokenKey> = {
  // Typography — families
  '--font-sans': "'Inter Variable', ui-sans-serif, system-ui, sans-serif",
  '--font-mono':
    "'Geist Mono Variable', ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace",

  // Typography — weights
  '--font-weight-normal': '400',
  '--font-weight-medium': '500',
  '--font-weight-semibold': '600',
  '--font-weight-bold': '700',

  // Typography — text sizes
  '--font-text-xs-size': '0.75rem',
  '--font-text-sm-size': '0.875rem',
  '--font-text-md-size': '1rem',
  '--font-text-lg-size': '1.125rem',

  // Typography — heading sizes
  '--font-heading-xs-size': '1rem',
  '--font-heading-sm-size': '1.125rem',
  '--font-heading-md-size': '1.25rem',
  '--font-heading-lg-size': '1.5rem',
  '--font-heading-xl-size': '1.875rem',
  '--font-heading-2xl-size': '2.25rem',
  '--font-heading-3xl-size': '3rem',

  // Typography — text line heights
  '--font-text-xs-line-height': '1rem',
  '--font-text-sm-line-height': '1.25rem',
  '--font-text-md-line-height': '1.5rem',
  '--font-text-lg-line-height': '1.75rem',

  // Typography — heading line heights
  '--font-heading-xs-line-height': '1.5rem',
  '--font-heading-sm-line-height': '1.75rem',
  '--font-heading-md-line-height': '1.75rem',
  '--font-heading-lg-line-height': '2rem',
  '--font-heading-xl-line-height': '2.25rem',
  '--font-heading-2xl-line-height': '2.5rem',
  '--font-heading-3xl-line-height': '3.5rem',

  // Border radius
  '--border-radius-xs': '2px',
  '--border-radius-sm': '4px',
  '--border-radius-md': '8px',
  '--border-radius-lg': '12px',
  '--border-radius-xl': '16px',
  '--border-radius-full': '9999px',

  // Border width
  '--border-width-regular': '1px',
};

// Theme-specific color/shadow tokens only.
type ColorTokens = Pick<ThemeTokens, ColorTokenKey>;

// ---------------------------------------------------------------------------
// Light theme — colors & shadows
// ---------------------------------------------------------------------------
const lightColorTokens: ColorTokens = {
  // Backgrounds — Cline-style neutrals (Radix Slate)
  '--color-background-primary': '#fdfcf9',
  '--color-background-secondary': '#f5f3ee',
  '--color-background-tertiary': '#eeebe5',
  '--color-background-inverse': '#292824',
  '--color-background-ghost': 'transparent',
  '--color-background-info': '#6e56cf',
  '--color-background-danger': '#e5484d',
  '--color-background-success': '#30a46c',
  '--color-background-warning': '#f76b15',
  '--color-background-disabled': '#eeebe5',

  // Text
  '--color-text-primary': '#292824',
  '--color-text-secondary': '#6b6962',
  '--color-text-tertiary': '#8b8d98',
  '--color-text-inverse': '#fcfcfd',
  '--color-text-ghost': '#8b8d98',
  '--color-text-info': '#6e56cf',
  '--color-text-danger': '#e5484d',
  '--color-text-success': '#30a46c',
  '--color-text-warning': '#f76b15',
  '--color-text-disabled': '#b9bbc6',

  // Borders
  '--color-border-primary': '#e8e4dc',
  '--color-border-secondary': '#dcd7cd',
  '--color-border-tertiary': '#cec8bd',
  '--color-border-inverse': '#292824',
  '--color-border-ghost': 'transparent',
  '--color-border-info': '#6e56cf',
  '--color-border-danger': '#e5484d',
  '--color-border-success': '#30a46c',
  '--color-border-warning': '#f76b15',
  '--color-border-disabled': '#e8e4dc',

  // Rings
  '--color-ring-primary': '#dcd7cd',
  '--color-ring-secondary': '#cdced6',
  '--color-ring-inverse': '#fcfcfd',
  '--color-ring-info': '#6e56cf',
  '--color-ring-danger': '#e5484d',
  '--color-ring-success': '#30a46c',
  '--color-ring-warning': '#f76b15',

  // Shadows
  '--shadow-hairline': '0 0 0 1px rgba(0, 0, 0, 0.05)',
  '--shadow-sm': '0 1px 2px 0 rgba(0, 0, 0, 0.05)',
  '--shadow-md': '0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -2px rgba(0, 0, 0, 0.1)',
  '--shadow-lg': '0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -4px rgba(0, 0, 0, 0.1)',
};

// ---------------------------------------------------------------------------
// Dark theme — colors & shadows
// ---------------------------------------------------------------------------
const darkColorTokens: ColorTokens = {
  // Backgrounds — Cline-style neutrals (Radix Slate dark)
  '--color-background-primary': '#1c1c1a',
  '--color-background-secondary': '#232320',
  '--color-background-tertiary': '#30302c',
  '--color-background-inverse': '#fcfcfd',
  '--color-background-ghost': 'transparent',
  '--color-background-info': '#8f7cec',
  '--color-background-danger': '#e54666',
  '--color-background-success': '#3dd68c',
  '--color-background-warning': '#ffb224',
  '--color-background-disabled': '#30302c',

  // Text
  '--color-text-primary': '#fcfcfd',
  '--color-text-secondary': '#b0b4ba',
  '--color-text-tertiary': '#777b84',
  '--color-text-inverse': '#121216',
  '--color-text-ghost': '#696e77',
  '--color-text-info': '#baa7ff',
  '--color-text-danger': '#ff949d',
  '--color-text-success': '#3dd68c',
  '--color-text-warning': '#ffb224',
  '--color-text-disabled': '#5a6169',

  // Borders
  '--color-border-primary': '#363630',
  '--color-border-secondary': '#363a3f',
  '--color-border-tertiary': '#30302c',
  '--color-border-inverse': '#fcfcfd',
  '--color-border-ghost': 'transparent',
  '--color-border-info': '#8f7cec',
  '--color-border-danger': '#e54666',
  '--color-border-success': '#3dd68c',
  '--color-border-warning': '#ffb224',
  '--color-border-disabled': '#363630',

  // Rings
  '--color-ring-primary': '#43484e',
  '--color-ring-secondary': '#363a3f',
  '--color-ring-inverse': '#121216',
  '--color-ring-info': '#8f7cec',
  '--color-ring-danger': '#e54666',
  '--color-ring-success': '#3dd68c',
  '--color-ring-warning': '#ffb224',

  // Shadows (darker for dark mode)
  '--shadow-hairline': '0 0 0 1px rgba(0, 0, 0, 0.2)',
  '--shadow-sm': '0 1px 2px 0 rgba(0, 0, 0, 0.2)',
  '--shadow-md': '0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -2px rgba(0, 0, 0, 0.2)',
  '--shadow-lg': '0 10px 15px -3px rgba(0, 0, 0, 0.3), 0 4px 6px -4px rgba(0, 0, 0, 0.2)',
};

// ---------------------------------------------------------------------------
// Aura theme — colors & shadows (dark variant)
// Ported from OpenChamber's Aura preset: purple-black surfaces, purple accent,
// mint/peach/cyan/coral status colors, monospace typography.
// ---------------------------------------------------------------------------
const auraColorTokens: ColorTokens = {
  // Backgrounds
  '--color-background-primary': '#15141b',
  '--color-background-secondary': '#1a1921',
  '--color-background-tertiary': '#201e2b',
  '--color-background-inverse': '#a277ff',
  '--color-background-ghost': 'transparent',
  '--color-background-info': '#82e2ff',
  '--color-background-danger': '#ff6767',
  '--color-background-success': '#61ffca',
  '--color-background-warning': '#ffca85',
  '--color-background-disabled': '#25232f',

  // Text
  '--color-text-primary': '#edecee',
  '--color-text-secondary': '#8a8282',
  '--color-text-tertiary': '#6d6d6d',
  '--color-text-inverse': '#15141b',
  '--color-text-ghost': '#8a8282',
  '--color-text-info': '#82e2ff',
  '--color-text-danger': '#ff6767',
  '--color-text-success': '#61ffca',
  '--color-text-warning': '#ffca85',
  '--color-text-disabled': '#525b68',

  // Borders
  '--color-border-primary': '#2d2b38',
  '--color-border-secondary': '#47415a',
  '--color-border-tertiary': '#4e496c',
  '--color-border-inverse': '#edecee',
  '--color-border-ghost': 'transparent',
  '--color-border-info': '#82e2ff',
  '--color-border-danger': '#ff6767',
  '--color-border-success': '#61ffca',
  '--color-border-warning': '#ffca85',
  '--color-border-disabled': '#2d2b38',

  // Rings
  '--color-ring-primary': '#47415a',
  '--color-ring-secondary': '#2d2b38',
  '--color-ring-inverse': '#15141b',
  '--color-ring-info': '#82e2ff',
  '--color-ring-danger': '#ff6767',
  '--color-ring-success': '#61ffca',
  '--color-ring-warning': '#ffca85',

  // Shadows (dark)
  '--shadow-hairline': '0 0 0 1px rgba(0, 0, 0, 0.2)',
  '--shadow-sm': '0 1px 2px 0 rgba(0, 0, 0, 0.2)',
  '--shadow-md': '0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -2px rgba(0, 0, 0, 0.2)',
  '--shadow-lg': '0 10px 15px -3px rgba(0, 0, 0, 0.3), 0 4px 6px -4px rgba(0, 0, 0, 0.2)',
};

// Aura is monospace-first — override the shared sans family.
const auraFontTokens: Partial<Pick<ThemeTokens, BaseTokenKey>> = {
  '--font-sans':
    'ui-monospace, "SFMono-Regular", "Menlo", "Cascadia Mono", "Segoe UI Mono", monospace',
  '--font-mono':
    'ui-monospace, "SFMono-Regular", "Menlo", "Cascadia Mono", "Segoe UI Mono", monospace',
};

// ---------------------------------------------------------------------------
// Merged token maps — used by applyThemeTokens() and buildMcpHostStyles()
// ---------------------------------------------------------------------------
export const lightTokens: ThemeTokens = { ...baseTokens, ...lightColorTokens };
export const darkTokens: ThemeTokens = { ...baseTokens, ...darkColorTokens };
export const auraTokens: ThemeTokens = { ...baseTokens, ...auraFontTokens, ...auraColorTokens };

// ---------------------------------------------------------------------------
// Theme registry — the set of selectable named themes.
// `variant` drives the .dark/.light class and colorScheme for anything outside
// the token system; `tokens` is the map applied to :root. Adding a future theme
// is a single entry here plus its token map above.
// ---------------------------------------------------------------------------
export type ThemeId = 'light' | 'dark' | 'aura';
export type ThemeVariant = 'light' | 'dark';

interface ThemeDefinition {
  variant: ThemeVariant;
  tokens: ThemeTokens;
}

export const themes: Record<ThemeId, ThemeDefinition> = {
  light: { variant: 'light', tokens: lightTokens },
  dark: { variant: 'dark', tokens: darkTokens },
  aura: { variant: 'dark', tokens: auraTokens },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Build the McpUiHostStyles object for MCP apps.
 *
 * For the built-in light/dark pair, color keys use light-dark() so a single
 * payload resolves correctly against the guest's color-scheme. Custom themes
 * (e.g. aura) share a variant with dark but carry their own palette and fonts,
 * so light-dark() can't express them — emit that theme's concrete token values
 * instead. Non-color keys always use the theme's own values so overrides like
 * Aura's monospace font family reach the guest.
 */
export function buildMcpHostStyles(themeId: ThemeId = 'light'): McpUiHostStyles {
  const tokens = (themes[themeId] ?? themes.light).tokens;
  const isBuiltinVariant = themeId === 'light' || themeId === 'dark';
  const variables: McpUiStyles = {} as McpUiStyles;
  for (const key of Object.keys(lightTokens) as McpUiStyleVariableKey[]) {
    if (key.startsWith('--color-') && isBuiltinVariant) {
      variables[key] = `light-dark(${lightTokens[key]}, ${darkTokens[key]})`;
    } else {
      variables[key] = tokens[key];
    }
  }
  return { variables };
}

/**
 * Resolve the current theme id from localStorage / system preference.
 * Best-effort pre-paint resolution; the authoritative preference lives in the
 * Electron settings store and is applied by ThemeContext once loaded.
 */
export function getResolvedTheme(): ThemeId {
  const useSystem = localStorage.getItem('use_system_theme') !== 'false';
  if (useSystem) {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  const stored = localStorage.getItem('theme');
  if (stored === 'aura') return 'aura';
  return stored === 'dark' ? 'dark' : 'light';
}

/**
 * Apply a theme's tokens to the document root as CSS custom properties.
 * When called without an argument, resolves the theme from localStorage.
 */
export function applyThemeTokens(theme?: ThemeId): void {
  const resolved = theme ?? getResolvedTheme();
  const { tokens } = themes[resolved] ?? themes.light;
  const root = document.documentElement;
  for (const [key, value] of Object.entries(tokens)) {
    root.style.setProperty(key, value);
  }
}
