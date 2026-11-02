// Shared UI state (Svelte 5 runes in a module).
import { api, on } from './api.js';
import { applyTheme, PALETTES } from './themes.js';

export const app = $state({
  ready: false,
  error: '',
  settings: null,
  systemDark: false, // OS preference (used when mode = 'system')
  info: { root: '', version: '', instancesDir: '' },
  instances: [],
  selectedId: null,
  deleteInstanceId: null,
  versions: [],
  javas: [],
  gpus: [],
  account: null,
  accounts: [],
  activeAccountId: null,
  accountSwitcherOpen: false,
  progress: {}, // instanceId -> { text, fraction }
  game: {},     // instanceId -> { state, message, needJava }
  blocked: {},  // instanceId -> files that must be downloaded manually (CurseForge)
  login: { status: 'idle', message: '' },
  dialog: null, // 'new' | 'settings' | 'account' | 'account-add' | 'delete-instance' | null
  toast: '',
});

let toastTimer;
export function toast(msg) {
  app.toast = String(msg);
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = ''), 5000);
}

export async function init() {
  // follow the OS light/dark preference
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  app.systemDark = mq.matches;
  mq.addEventListener('change', (e) => (app.systemDark = e.matches));

  try {
    app.settings = await api.settings();
    // older versions had a separate "midnight" theme -> Lavender in dark mode
    if (!PALETTES.some((p) => p.key === app.settings.theme)) {
      const wasMidnight = app.settings.theme === 'midnight';
      app.settings = { ...app.settings, theme: 'lavender', mode: wasMidnight ? 'dark' : app.settings.mode };
      await api.saveSettings($state.snapshot(app.settings));
    }
    applyTheme(app.settings.theme, app.settings.mode, app.systemDark);
    app.info = await api.info();
    app.instances = await api.instances();
    app.selectedId = app.instances[0]?.id ?? null;
    await refreshAccounts();
    app.gpus = await api.gpus();
  } catch (e) {
    app.error = String(e);
  }
  app.ready = true;

  api.java().then((j) => (app.javas = j)).catch(() => {});
  loadVersions();

  await on('progress', (p) => {
    app.progress[p.id] = { text: p.text, fraction: p.fraction };
  });
  await on('game-state', (s) => {
    app.game[s.id] = s;
    if (s.state === 'error') toast(s.message);
    if (s.state === 'idle' || s.state === 'error' || s.state === 'running') refreshInstances();
  });
  await on('modpack-blocked', (p) => {
    app.blocked[p.id] = p.files;
  });
  await on('login', (l) => {
    app.login = { status: l.status, message: l.message };
    if (l.status === 'done') refreshAccounts();
  });
}

function applyAccountsSnapshot(snapshot) {
  // Ignore legacy Microsoft accounts while Microsoft sign-in is disabled.
  app.accounts = (snapshot.accounts ?? []).filter((account) => account.provider !== 'microsoft');
  app.activeAccountId = app.accounts.some((account) => account.id === snapshot.activeAccountId) ? snapshot.activeAccountId : null;
  app.account = app.accounts.find((a) => a.id === app.activeAccountId) ?? null;
  for (const account of app.accounts) {
    if (account.skinUrl) continue;
    api.accountSkin(account.id).then((skinUrl) => {
      if (!skinUrl) return;
      const saved = app.accounts.find((a) => a.id === account.id);
      if (saved) saved.skinUrl = skinUrl;
      if (app.account?.id === account.id) app.account.skinUrl = skinUrl;
    }).catch(() => {});
  }
}

export async function refreshAccounts() {
  try {
    applyAccountsSnapshot(await api.accounts());
  } catch (e) {
    toast(e);
  }
}

export async function loadVersions() {
  if (app.versions.length) return;
  try {
    app.versions = await api.mcVersions();
  } catch (e) {
    toast('Could not load Minecraft versions: ' + e);
  }
}

export async function refreshInstances() {
  try {
    app.instances = await api.instances();
    if (app.selectedId !== null && !app.instances.some((i) => i.id === app.selectedId)) {
      app.selectedId = app.instances[0]?.id ?? null;
    }
  } catch (e) {
    toast(e);
  }
}

export function selectInstance(id) {
  app.selectedId = id;
}

export async function saveSettings(patch) {
  app.settings = { ...app.settings, ...patch };
  applyTheme(app.settings.theme, app.settings.mode, app.systemDark);
  try {
    await api.saveSettings($state.snapshot(app.settings));
  } catch (e) {
    toast(e);
  }
}

export async function createInstance(input) {
  const inst = await api.createInstance(input);
  await refreshInstances();
  app.selectedId = inst.id;
  return inst;
}

export async function updateInstance(inst) {
  await api.updateInstance($state.snapshot(inst));
  await refreshInstances();
}

export async function deleteInstance(id, deleteFiles) {
  await api.deleteInstance(id, deleteFiles);
  await refreshInstances();
}

export async function play(id) {
  app.game[id] = { state: 'installing', message: '', needJava: null };
  app.progress[id] = { text: 'Starting…', fraction: 0 };
  try {
    await api.play(id);
  } catch (e) {
    app.game[id] = { state: 'error', message: String(e), needJava: null };
    toast(e);
  }
}

export async function stop(id) {
  try {
    await api.stop(id);
  } catch (e) {
    toast(e);
  }
}

export async function login(provider = 'microsoft', username = '', password = '', totp = '') {
  app.login = { status: 'waiting', message: '' };
  try {
    await api.login(provider, username, password, totp);
  } catch (e) {
    app.login = { status: 'error', message: String(e) };
  }
}

export async function cancelLogin() {
  await api.cancelLogin();
  app.login = { status: 'idle', message: '' };
}

export async function selectAccount(id) {
  try {
    applyAccountsSnapshot(await api.selectAccount(id));
    app.accountSwitcherOpen = false;
  } catch (e) {
    toast(e);
  }
}

export async function logout(id) {
  try {
    applyAccountsSnapshot(await api.removeAccount(id));
  } catch (e) {
    toast(e);
  }
}

export async function refreshJava() {
  try {
    app.javas = await api.java();
  } catch (e) {
    toast(e);
  }
}
