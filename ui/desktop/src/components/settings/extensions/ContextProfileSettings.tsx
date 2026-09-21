import { useId, useState } from 'react';
import { useConfig } from '../../ConfigContext';
import { defineMessages, useIntl } from '../../../i18n';

const PROFILE_KEY = 'KARMX_PROMPT_PROFILE';

const messages = defineMessages({
  title: { id: 'contextProfile.title', defaultMessage: 'Startup context' },
  description: {
    id: 'contextProfile.description',
    defaultMessage: 'Choose how much context the agent loads before your first message.',
  },
  automatic: { id: 'contextProfile.automatic', defaultMessage: 'Use provider default' },
  compact: { id: 'contextProfile.compact', defaultMessage: 'Compact — lower context' },
  full: { id: 'contextProfile.full', defaultMessage: 'Full — all enabled extensions' },
  custom: { id: 'contextProfile.custom', defaultMessage: 'Unrecognized saved profile' },
  detail: {
    id: 'contextProfile.detail',
    defaultMessage:
      'Compact uses a shorter prompt and a limited tool set. Browser control and Jev screen tasks remain allowed when enabled; other extensions, including skills, may be excluded.',
  },
  override: {
    id: 'contextProfile.override',
    defaultMessage:
      'A custom extension allowlist is configured and takes precedence over the profile’s tool selection.',
  },
  restart: {
    id: 'contextProfile.restart',
    defaultMessage:
      'Shared with the CLI. Restart the app or CLI session after changing this setting. Environment overrides take precedence. Jev screen tasks require macOS permissions and a TypeSafe API key.',
  },
  saved: { id: 'contextProfile.saved', defaultMessage: 'Saved. Restart to apply consistently.' },
  failed: {
    id: 'contextProfile.failed',
    defaultMessage: 'Could not save the context profile. Try again.',
  },
});

export function ContextProfileSettings() {
  const intl = useIntl();
  const id = useId();
  const { config, upsert, remove } = useConfig();
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<'saved' | 'failed' | null>(null);
  const raw = config[PROFILE_KEY];
  const normalized = typeof raw === 'string' ? raw.trim().toLowerCase() : null;
  const value =
    raw == null
      ? 'auto'
      : normalized === 'compact' || normalized === 'small'
        ? 'compact'
        : normalized === '' || normalized === 'default' || normalized === 'full'
          ? 'default'
          : 'unknown';
  const allowlist = config.KARMX_EXTENSIONS_ALLOWLIST;
  const hasAllowlist = typeof allowlist === 'string' && allowlist.trim().length > 0;

  const save = async (profile: string) => {
    setSaving(true);
    setResult(null);
    try {
      if (profile === 'auto') await remove(PROFILE_KEY, false);
      else await upsert(PROFILE_KEY, profile, false);
      setResult('saved');
    } catch {
      setResult('failed');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mb-6 rounded-xl border border-borderSubtle bg-background-muted p-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <label htmlFor={id} className="block text-sm font-medium text-text-primary">
            {intl.formatMessage(messages.title)}
          </label>
          <p id={`${id}-description`} className="text-sm text-text-secondary">
            {intl.formatMessage(messages.description)}
          </p>
        </div>
        <select
          id={id}
          value={value}
          disabled={saving}
          aria-describedby={`${id}-description ${id}-detail ${id}-restart`}
          onChange={(event) => void save(event.target.value)}
          className="max-w-full rounded-lg border border-borderSubtle bg-background-default px-3 py-2 text-sm text-text-primary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 disabled:opacity-50"
        >
          {value === 'unknown' && (
            <option value="unknown" disabled>
              {intl.formatMessage(messages.custom)}
            </option>
          )}
          <option value="auto">{intl.formatMessage(messages.automatic)}</option>
          <option value="compact">{intl.formatMessage(messages.compact)}</option>
          <option value="default">{intl.formatMessage(messages.full)}</option>
        </select>
      </div>
      <p id={`${id}-detail`} className="mt-3 text-xs leading-relaxed text-text-secondary">
        {intl.formatMessage(messages.detail)}
      </p>
      {hasAllowlist && (
        <p className="mt-2 text-xs text-text-primary">{intl.formatMessage(messages.override)}</p>
      )}
      <p id={`${id}-restart`} className="mt-2 text-xs leading-relaxed text-text-secondary">
        {intl.formatMessage(messages.restart)}
      </p>
      {result && (
        <p
          role={result === 'failed' ? 'alert' : 'status'}
          className="mt-3 text-xs text-text-primary"
        >
          {intl.formatMessage(messages[result])}
        </p>
      )}
    </div>
  );
}
