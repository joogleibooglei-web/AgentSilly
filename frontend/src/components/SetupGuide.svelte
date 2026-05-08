<script lang="ts">
  import type { PlatformInfo } from '../lib/stores/connection';

  interface Props {
    platformInfo: PlatformInfo;
  }

  let { platformInfo }: Props = $props();

  const RELEASES_URL = 'https://github.com/joogleibooglei-web/AgentSilly/releases';

  function getBinaryName(info: PlatformInfo): string {
    const ext = info.platform === 'win32' ? '.exe' : '';
    return `eni-sidecar-${info.platform}-${info.arch}${ext}`;
  }

  function getPlatformLabel(info: PlatformInfo): string {
    const platformNames: Record<string, string> = {
      darwin: 'macOS',
      win32: 'Windows',
      linux: 'Linux',
      unknown: 'Unknown OS',
    };
    const archNames: Record<string, string> = {
      arm64: 'ARM64 (Apple Silicon)',
      x64: 'x86_64',
    };
    return `${platformNames[info.platform] ?? info.platform} / ${archNames[info.arch] ?? info.arch}`;
  }
</script>

<div class="setup-guide">
  <div class="setup-icon">⚠️</div>
  <h3 class="setup-title">Sidecar Binary Not Available</h3>

  <p class="setup-text">
    ENI couldn't connect to the sidecar process. This usually means the binary
    isn't installed yet or failed to download automatically.
  </p>

  <div class="platform-badge">
    <span class="platform-label">Detected platform:</span>
    <span class="platform-value">{getPlatformLabel(platformInfo)}</span>
  </div>

  <div class="setup-steps">
    <p class="step-heading">To get started:</p>
    <ol class="steps-list">
      <li>
        Download <code class="binary-name">{getBinaryName(platformInfo)}</code> from
        <a href={RELEASES_URL} target="_blank" rel="noopener noreferrer" class="releases-link">
          GitHub Releases ↗
        </a>
      </li>
      <li>
        Place the binary in the extension's <code class="binary-name">bin/</code> folder
      </li>
      <li>
        Reload the extension (or restart SillyTavern) to trigger the sidecar spawn
      </li>
    </ol>
  </div>

  <p class="setup-hint">
    If you're on an unsupported platform, you can build from source — see the repo README.
  </p>
</div>

<style>
  .setup-guide {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 24px 20px;
    margin: 16px 8px;
    background: var(--bg-surface, #1f1f36);
    border: 1px solid var(--border, #3a3a5c);
    border-radius: 8px;
    animation: fadeIn 300ms ease;
  }

  .setup-icon {
    font-size: 28px;
    margin-bottom: 12px;
  }

  .setup-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text, #e0e0e0);
    margin: 0 0 10px 0;
  }

  .setup-text {
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-muted, #6b6b8a);
    margin: 0 0 14px 0;
    max-width: 320px;
  }

  .platform-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--bg-elevated, #252542);
    border: 1px solid var(--border, #3a3a5c);
    border-radius: 4px;
    margin-bottom: 16px;
  }

  .platform-label {
    font-size: 10px;
    color: var(--text-muted, #6b6b8a);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .platform-value {
    font-size: 11px;
    font-weight: 500;
    color: var(--accent, #7c5cfc);
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
  }

  .setup-steps {
    text-align: left;
    width: 100%;
    max-width: 340px;
    margin-bottom: 14px;
  }

  .step-heading {
    font-size: 11px;
    font-weight: 600;
    color: var(--text, #e0e0e0);
    margin: 0 0 8px 0;
  }

  .steps-list {
    padding-left: 20px;
    margin: 0;
    font-size: 11.5px;
    line-height: 1.6;
    color: var(--text, #e0e0e0);
  }

  .steps-list li {
    margin-bottom: 6px;
  }

  .binary-name {
    background: var(--bg-elevated, #252542);
    padding: 1px 5px;
    border-radius: 3px;
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
    font-size: 10.5px;
    color: var(--accent, #7c5cfc);
  }

  .releases-link {
    color: var(--accent, #7c5cfc);
    text-decoration: none;
    font-weight: 500;
  }

  .releases-link:hover {
    text-decoration: underline;
  }

  .setup-hint {
    font-size: 10.5px;
    color: var(--text-muted, #6b6b8a);
    margin: 0;
    font-style: italic;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
