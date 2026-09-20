import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AgentSettingsSection from './AgentSettingsSection';
import { acpReadConfig, acpUpsertConfig, acpRemoveConfig } from '../../acp/config';
import { acpListSettingsProviderDetails } from '../../acp/providers';
import { IntlTestWrapper } from '../../i18n/test-utils';
vi.mock('../../acp/config', () => ({
  acpReadConfig: vi.fn(),
  acpUpsertConfig: vi.fn(),
  acpRemoveConfig: vi.fn(),
}));
vi.mock('../../acp/providers', () => ({ acpListSettingsProviderDetails: vi.fn() }));
vi.mock('../ConfigContext', () => ({ useConfig: () => ({ extensionsList: [] }) }));
function mount() {
  return render(
    <MemoryRouter>
      <IntlTestWrapper>
        <AgentSettingsSection />
      </IntlTestWrapper>
    </MemoryRouter>
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(acpReadConfig).mockResolvedValue(null);
  vi.mocked(acpListSettingsProviderDetails).mockResolvedValue([]);
  vi.mocked(acpUpsertConfig).mockResolvedValue();
  vi.mocked(acpRemoveConfig).mockResolvedValue();
});
describe('Agent settings', () => {
  it('keeps backend defaults untouched until the user saves', async () => {
    mount();
    await screen.findByRole('button', { name: 'Save agent settings' });
    expect(acpUpsertConfig).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('switch'));
    fireEvent.click(screen.getByRole('button', { name: 'Save agent settings' }));
    await screen.findByText('Saved to the Rust backend');
    expect(acpUpsertConfig).toHaveBeenCalledWith('KARMX_CONTEXT_AUTO_RETRIEVE', true);
    expect(acpRemoveConfig).toHaveBeenCalledWith('GOOSE_COMPACTION_THRESHOLDS', false);
  });
  it('rejects unordered thresholds before writing any settings', async () => {
    mount();
    fireEvent.change(await screen.findByLabelText('Prepare, apply, hard limit (tokens)'), {
      target: { value: '500,200,100' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save agent settings' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('non-decreasing');
    expect(acpUpsertConfig).not.toHaveBeenCalled();
  });
  it('does not replace unknown configuration after a failed read', async () => {
    vi.mocked(acpReadConfig).mockRejectedValue(new Error('Backend unavailable'));
    mount();
    expect(await screen.findByRole('alert')).toHaveTextContent('Backend unavailable');
    expect(screen.getByRole('button', { name: 'Save agent settings' })).toBeDisabled();
    expect(acpUpsertConfig).not.toHaveBeenCalled();
  });
});
