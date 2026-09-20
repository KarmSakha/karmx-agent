import { useEffect, useState } from 'react';
import { Link } from 'react-router';
import { useConfig } from '../ConfigContext';
import { Cpu, Layers, ScanSearch, RefreshCw, Save } from 'lucide-react';
import { acpReadConfig, acpUpsertConfig, acpRemoveConfig } from '../../acp/config';
import { acpListSettingsProviderDetails } from '../../acp/providers';
import type { ProviderDetails } from '../../types/providers';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';

const KEYS = [
  'KARMX_SIDEKICK_PROVIDER',
  'KARMX_SIDEKICK_MODEL',
  'KARMX_CONTEXT_AUTO_RETRIEVE',
  'GOOSE_COMPACTION_THRESHOLDS',
] as const;

export default function AgentSettingsSection() {
  const { extensionsList } = useConfig();
  const [configurationLoaded, setConfigurationLoaded] = useState(false);
  const [providers, setProviders] = useState<ProviderDetails[]>([]);
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');
  const [retrieve, setRetrieve] = useState(false);
  const [thresholds, setThresholds] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let active = true;
    Promise.all([
      acpListSettingsProviderDetails(),
      Promise.all(KEYS.map((key) => acpReadConfig(key))),
    ])
      .then(([items, values]) => {
        if (!active) return;
        setProviders(items.filter((item) => item.is_configured));
        setProvider(typeof values[0] === 'string' ? values[0] : '');
        setModel(typeof values[1] === 'string' ? values[1] : '');
        setRetrieve(values[2] === true);
        setThresholds(typeof values[3] === 'string' ? values[3] : '');
        setConfigurationLoaded(true);
      })
      .catch((err) => active && setError(String(err)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, []);

  async function save() {
    setError('');
    setSaved(false);
    if (thresholds.trim()) {
      const values = thresholds.split(',').map(Number);
      if (
        ![2, 3].includes(values.length) ||
        values.some((value) => !Number.isSafeInteger(value) || value <= 0) ||
        values.some((value, index) => index > 0 && value < values[index - 1])
      ) {
        setError(
          'Enter two or three non-decreasing positive token counts: prepare, optional apply, and hard limit.'
        );
        return;
      }
    }
    setSaving(true);
    try {
      // Save the model before enabling its provider so a partially completed save cannot pair it with an old model.
      await acpUpsertConfig(KEYS[1], model.trim());
      await acpUpsertConfig(KEYS[0], provider);
      await acpUpsertConfig(KEYS[2], retrieve);
      if (thresholds.trim()) await acpUpsertConfig(KEYS[3], thresholds.trim());
      else await acpRemoveConfig(KEYS[3], false);
      setSaved(true);
    } catch (err) {
      setError(`Could not save all settings. Reload to check the current values. ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  if (loading)
    return (
      <p role="status" className="p-6 text-text-secondary">
        Reading agent configuration…
      </p>
    );
  return (
    <section className="karmx-agent-settings">
      <div className="karmx-agent-heading">
        <span className="karmx-eyebrow">RUST ENGINE</span>
        <h2>One agent. More possibilities.</h2>
        <p>
          These settings are shared with the terminal. Environment overrides take precedence. Start
          a new chat after changing the sidekick.
        </p>
      </div>
      <div className="karmx-agent-card">
        <Layers size={21} />
        <div>
          <h3>Local fusion</h3>
          <p>
            Let a dedicated sidekick handle implementation while your lead model keeps the bigger
            picture.
          </p>
          <div className="karmx-agent-fields">
            <label>
              Sidekick provider
              <select
                value={provider}
                onChange={(event) => {
                  setProvider(event.target.value);
                  setModel('');
                  setSaved(false);
                }}
              >
                <option value="">Same as lead</option>
                {providers.map((item) => (
                  <option key={item.name} value={item.name}>
                    {item.metadata.display_name}{item.is_available ? '' : ' (unavailable)'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Sidekick model
              <input
                list="karmx-sidekick-models"
                value={model}
                onChange={(event) => {
                  setModel(event.target.value);
                  setSaved(false);
                }}
                placeholder="Same as lead"
              />
              <datalist id="karmx-sidekick-models">
                {providers
                  .find((item) => item.name === provider)
                  ?.metadata.known_models.map((item) => (
                    <option key={item.name} value={item.name} />
                  ))}
              </datalist>
            </label>
          </div>
          <p className="karmx-agent-help">
            Model suggestions come from the backend. Custom model IDs are supported. Enable the
            summon extension to make the sidekick tool available.
          </p>
        </div>
      </div>
      <div className="karmx-agent-card">
        <ScanSearch size={21} />
        <div>
          <h3>Automatic code context</h3>
          <p>
            Retrieve relevant code from the selected project before each message. The same setting
            applies to the terminal.
          </p>
          <div className="karmx-agent-toggle">
            <label htmlFor="auto-retrieve">Retrieve project context on send</label>
            <Switch
              id="auto-retrieve"
              checked={retrieve}
              onCheckedChange={(value) => {
                setRetrieve(value);
                setSaved(false);
              }}
            />
          </div>
        </div>
      </div>
      <div className="karmx-agent-card">
        <RefreshCw size={21} />
        <div>
          <h3>Predictive compaction</h3>
          <p>
            The Rust engine prepares summaries before the context window fills. Leave this blank to
            use its model-aware defaults.
          </p>
          <label className="karmx-agent-label">
            Prepare, apply, hard limit (tokens)
            <input
              value={thresholds}
              onChange={(event) => {
                setThresholds(event.target.value);
                setSaved(false);
              }}
              placeholder="Automatic"
            />
          </label>
        </div>
      </div>
      <div className="karmx-agent-card">
        <Cpu size={21} />
        <div>
          <h3>Connected capabilities</h3>
          <div className="karmx-extension-statuses">
            {extensionsList.map((extension) => (
              <span key={extension.name} className={extension.enabled ? 'is-enabled' : ''}>
                {extension.name.replaceAll('_', ' ')}
                <small>{extension.enabled ? 'Enabled' : 'Disabled'}</small>
              </span>
            ))}
          </div>
          <Link className="karmx-manage-link" to="/extensions">
            Manage extensions →
          </Link>
          <p>
            Browser preview, computer control, code retrieval, and context reranking run as native
            extensions. Manage them in Extensions. Skills, recipes, scheduling, model reasoning, and
            tool approvals have dedicated controls in the sidebar and settings.
          </p>
          <p className="karmx-agent-help">
            Use Enhance in the composer, or Ctrl+P, to refine a draft with your current model before
            sending.
          </p>
        </div>
      </div>
      {error && (
        <p role="alert" className="text-text-danger">
          {error}
        </p>
      )}
      <div className="karmx-agent-save">
        <Button onClick={save} disabled={saving || !configurationLoaded}>
          <Save size={16} />
          {saving ? 'Saving…' : 'Save agent settings'}
        </Button>
        {saved && <span role="status">Saved to the Rust backend</span>}
      </div>
    </section>
  );
}
