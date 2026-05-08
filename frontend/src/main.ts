/**
 * SillyTavern Extension Bootstrap
 *
 * Registers the ENI World Builder as a SillyTavern extension,
 * creates the panel toggle button, and mounts the Svelte App.
 */
import App from './App.svelte';
import { mount, unmount } from 'svelte';
import { getWsClient, resetWsClient } from './lib/ws/client';

const EXTENSION_NAME = 'eni-world-builder';
const PANEL_TOGGLE_ICON = '🌍';

let appInstance: Record<string, unknown> | null = null;
let container: HTMLElement | null = null;

/**
 * Creates the panel container element and appends it to the document body.
 */
function createPanelContainer(): HTMLElement {
  const el = document.createElement('div');
  el.id = `${EXTENSION_NAME}-panel`;
  el.classList.add('wb-root');
  document.body.appendChild(el);
  return el;
}

/**
 * Creates the toggle button in the ST UI extensions menu area.
 */
function createToggleButton(): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.id = `${EXTENSION_NAME}-toggle`;
  btn.classList.add('wb-toggle-btn');
  btn.title = 'Toggle World Builder Panel';
  btn.textContent = PANEL_TOGGLE_ICON;
  btn.addEventListener('click', togglePanel);
  return btn;
}

/**
 * Toggles the panel open/closed.
 */
function togglePanel(): void {
  if (!container) return;
  container.classList.toggle('wb-panel-open');
}

/**
 * Mounts the Svelte app into the panel container.
 */
function mountApp(): void {
  if (appInstance || !container) return;
  appInstance = mount(App, { target: container });
}

/**
 * Unmounts the Svelte app and cleans up.
 */
function destroyApp(): void {
  if (appInstance) {
    unmount(appInstance);
    appInstance = null;
  }
  if (container) {
    container.remove();
    container = null;
  }
}

/**
 * Extension init function — called by SillyTavern when the extension loads.
 */
function init(): void {
  // Create the panel container
  container = createPanelContainer();

  // Create the toggle button and insert into ST UI
  const toggleBtn = createToggleButton();
  const extensionMenu = document.getElementById('extensionsMenu');
  if (extensionMenu) {
    extensionMenu.appendChild(toggleBtn);
  } else {
    // Fallback: append to body if ST menu not found
    document.body.appendChild(toggleBtn);
  }

  // Mount the Svelte app
  mountApp();

  // Connect to sidecar WebSocket
  const wsClient = getWsClient();
  wsClient.connect();
}

/**
 * Extension exit function — called by SillyTavern when the extension unloads.
 */
function exit(): void {
  // Disconnect WebSocket
  resetWsClient();

  destroyApp();
  const toggleBtn = document.getElementById(`${EXTENSION_NAME}-toggle`);
  if (toggleBtn) toggleBtn.remove();
}

// Export for ST extension system
export { init, exit, EXTENSION_NAME };
