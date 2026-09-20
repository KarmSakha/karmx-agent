/**
 * Hub Component
 *
 * The empty-chat landing screen. Visually it's "Pair with no messages yet" —
 * a large time + greeting above a centered, narrower ChatInput. Submitting
 * creates a session and navigates to /pair so the rest of the chat lifecycle
 * lives there.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { defineMessages, useIntl } from '../i18n';
import { ArrowUpRight, Code2, FolderCode, ScanSearch, Wrench } from 'lucide-react';
import { AppEvents } from '../constants/events';
import ChatInput from './ChatInput';
import { ChatInputCard } from './ChatInputCard';
import { ChatState } from '../types/chatState';
import 'react-toastify/dist/ReactToastify.css';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import { getEffectiveWorkingDir, getInitialWorkingDir } from '../utils/workingDir';
import { createSession } from '../sessions';
import LoadingGoose from './LoadingGoose';
import { UserInput } from '../types/message';
import {
  createNextChatExtensionDraft,
  selectNextChatExtensions,
  type NextChatExtensionDraft,
} from '../utils/nextChatExtensions';
import { formatAcpError } from '../acp/errors';
import { toastError } from '../toasts';
import { formatClockDisplay } from '../utils/timeUtils';

const i18n = defineMessages({
  goodMorning: { id: 'hub.goodMorning', defaultMessage: 'Good morning' },
  goodAfternoon: { id: 'hub.goodAfternoon', defaultMessage: 'Good afternoon' },
  goodEvening: { id: 'hub.goodEvening', defaultMessage: 'Good evening' },
});

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const interval = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(interval);
  }, []);

  return formatClockDisplay(now);
}

export default function Hub({
  setView,
  draftRef,
}: {
  setView: (view: View, viewOptions?: ViewOptions) => void;
  /** Unsent input of this screen, kept above the route outlet across the unmount. */
  draftRef: RefObject<string>;
}) {
  const intl = useIntl();
  const { extensionsList } = useConfig();
  const [workingDir, setWorkingDir] = useState(getInitialWorkingDir());
  const userSelectedWorkingDirRef = useRef(false);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const [nextChatExtensionDraft, setNextChatExtensionDraft] =
    useState<NextChatExtensionDraft | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const { time, meridiem, hour } = useClock();
  const [starterPrompt, setStarterPrompt] = useState('');

  const chooseStarter = (prompt: string) => {
    const next = draftRef.current.trim() ? `${draftRef.current}\n\n${prompt}` : prompt;
    draftRef.current = next;
    setStarterPrompt(next);
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  // Re-resolve the working dir on mount: GOOSE_WORKING_DIR is fixed at window
  // creation, so a configured remote directory may have changed since then.
  useEffect(() => {
    let active = true;
    void getEffectiveWorkingDir().then((dir) => {
      if (active && !userSelectedWorkingDirRef.current) setWorkingDir(dir);
    });
    return () => {
      active = false;
    };
  }, []);

  const greeting = useMemo(() => {
    if (hour < 12) return intl.formatMessage(i18n.goodMorning);
    if (hour < 18) return intl.formatMessage(i18n.goodAfternoon);
    return intl.formatMessage(i18n.goodEvening);
  }, [intl, hour]);

  const draftForMenu = useMemo(
    () => nextChatExtensionDraft ?? createNextChatExtensionDraft(extensionsList),
    [extensionsList, nextChatExtensionDraft]
  );

  // rAF is more reliable than autoFocus across async render boundaries.
  useEffect(() => {
    const frameId = requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => cancelAnimationFrame(frameId);
  }, []);

  const handleNextChatExtensionDraftChange = useCallback((draft: NextChatExtensionDraft) => {
    setNextChatExtensionDraft(draft);
  }, []);

  const handleWorkingDirChange = useCallback((dir: string) => {
    userSelectedWorkingDirRef.current = true;
    setWorkingDir(dir);
  }, []);

  const handleSubmit = async (input: UserInput) => {
    const { msg: userMessage, images } = input;
    if (!(images.length > 0 || userMessage.trim()) || isCreatingSession) return;

    const draftAtSubmit = draftRef.current;
    setIsCreatingSession(true);

    try {
      const selectedExtensions = nextChatExtensionDraft
        ? selectNextChatExtensions(extensionsList, nextChatExtensionDraft)
        : [];
      const sessionOptions =
        selectedExtensions.length > 0
          ? { extensionConfigs: selectedExtensions }
          : { allExtensions: extensionsList };

      // Resolve the effective directory at submit time: the IPC lookup may still
      // be pending when the user submits, and an explicit pick must win.
      const dir = userSelectedWorkingDirRef.current ? workingDir : await getEffectiveWorkingDir();
      const session = await createSession(dir, sessionOptions);
      setNextChatExtensionDraft(null);

      window.dispatchEvent(new CustomEvent(AppEvents.SESSION_CREATED));
      window.dispatchEvent(
        new CustomEvent(AppEvents.ADD_ACTIVE_SESSION, {
          detail: { sessionId: session.id, initialMessage: { msg: userMessage, images } },
        })
      );

      // The draft is this screen's own, so it is dropped once the session exists.
      // Comparing it against the value at submit leaves an edit made while the
      // session was starting alone, including one that emptied the input.
      if (draftRef.current === draftAtSubmit) {
        draftRef.current = '';
      }

      setView('pair', {
        disableAnimation: true,
        resumeSessionId: session.id,
        initialMessage: { msg: userMessage, images },
      });
    } catch (error) {
      console.error('Failed to create session:', error);
      toastError({ title: "Couldn't start chat", msg: formatAcpError(error) });
      setIsCreatingSession(false);
    }
  };

  return (
    <div className="karmx-workspace">
      <header className="karmx-workspace-header">
        <span className="karmx-eyebrow">WORKSPACE</span>
        <span className="karmx-clock">
          {time} {meridiem}
        </span>
      </header>
      <div className="karmx-workspace-content">
        <div className="karmx-intro">
          <div className="karmx-eyebrow karmx-greeting">{greeting}. Let's make something.</div>
          <h1>
            Your next idea.
            <br />
            <span>Within reach.</span>
          </h1>
          <p>A focused space to understand, build, and improve your code.</p>
        </div>
        <div className="karmx-project-context" title={workingDir}>
          <FolderCode size={16} aria-hidden="true" />
          <span>{workingDir.split(/[\\/]/).filter(Boolean).pop() || workingDir}</span>
          <span className="karmx-project-caption">Current project</span>
        </div>
        <ChatInputCard className="karmx-home-composer">
          <ChatInput
            sessionId={null}
            draftRef={draftRef}
            handleSubmit={handleSubmit}
            chatState={isCreatingSession ? ChatState.LoadingConversation : ChatState.Idle}
            onStop={() => {}}
            suggestedPrompt={starterPrompt}
            placeholder="What would you like to work on?"
            setView={setView}
            totalTokens={0}
            accumulatedInputTokens={0}
            accumulatedOutputTokens={0}
            droppedFiles={[]}
            onFilesProcessed={() => {}}
            messages={[]}
            disableAnimation={false}
            workingDir={workingDir}
            onWorkingDirChange={handleWorkingDirChange}
            inputRef={inputRef}
            nextChatExtensionDraft={draftForMenu}
            onNextChatExtensionDraftChange={handleNextChatExtensionDraftChange}
          />
        </ChatInputCard>
        <div className="karmx-starters" aria-label="Start with a task">
          {[
            {
              icon: ScanSearch,
              title: 'Understand',
              detail: 'Find your way through the code',
              prompt:
                'Explore this project and explain its architecture, entry points, and how the main pieces work together. Do not make changes yet.',
            },
            {
              icon: Code2,
              title: 'Build',
              detail: 'Turn an idea into working code',
              prompt:
                'Help me build a new feature in this project. First inspect the existing patterns, then ask me what I want to create.',
            },
            {
              icon: Wrench,
              title: 'Improve',
              detail: 'Make what exists work better',
              prompt:
                'Review this project for meaningful improvements. Identify concrete bugs and usability issues, explain your findings, and propose a focused plan before editing.',
            },
          ].map(({ icon: Icon, title, detail, prompt }) => (
            <button
              key={title}
              className="karmx-starter"
              disabled={isCreatingSession}
              onClick={() => chooseStarter(prompt)}
            >
              <div className="karmx-starter-top">
                <Icon size={18} aria-hidden="true" />
                <ArrowUpRight size={15} aria-hidden="true" />
              </div>
              <strong>{title}</strong>
              <span>{detail}</span>
            </button>
          ))}
        </div>
        <div className="karmx-workspace-note">
          <span>YOUR TOOLS. YOUR MODELS. YOUR WORKSPACE.</span>
          <span>Built for the way you think.</span>
        </div>
      </div>

      {isCreatingSession && (
        <div className="absolute bottom-4 left-4 z-20 pointer-events-none">
          <LoadingGoose chatState={ChatState.LoadingConversation} />
        </div>
      )}
    </div>
  );
}
