import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// thin wrappers around the Rust commands (src-tauri/src/commands.rs)
export const api = {
  info: () => invoke('app_info'),
  settings: () => invoke('get_settings'),
  saveSettings: (settings) => invoke('save_settings', { settings }),

  instances: () => invoke('list_instances'),
  createInstance: (input) => invoke('create_instance', { input }),
  updateInstance: (instance) => invoke('update_instance', { instance }),
  deleteInstance: (id, deleteFiles) => invoke('delete_instance', { id, deleteFiles }),

  mcVersions: () => invoke('list_mc_versions'),
  loaderVersions: (kind, mc) => invoke('list_loader_versions', { kind, mc }),
  java: () => invoke('detect_java'),
  addJava: (path) => invoke('add_custom_java', { path }),
  gpus: () => invoke('gpu_info'),

  play: (id, profile = null) => invoke('play', { id, profile }),
  stop: (id) => invoke('stop', { id }),

  accounts: () => invoke('account_status'),
  accountSkin: (id) => invoke('account_skin', { id }),
  selectAccount: (id) => invoke('select_account', { id }),
  removeAccount: (id) => invoke('remove_account', { id }),
  login: (provider, username = '', password = '', totp = '') => invoke('login_start', { provider, username, password, totp }),
  cancelLogin: () => invoke('login_cancel'),

  openFolder: (id, kind) => invoke('open_folder', { id, kind }),
  openUrl: (url) => invoke('open_url', { url }),
  pickFolder: () => invoke('pick_folder'),
  pickImage: () => invoke('pick_image'),
  pickMusicFiles: () => invoke('pick_music_files'),

  searchPacks: (source, query, offset) => invoke('search_modpacks', { source, query, offset }),
  packVersions: (source, projectId) => invoke('modpack_versions', { source, projectId }),
  installPack: (req) => invoke('install_modpack', { req }),
  pickModpackFile: () => invoke('pick_modpack_file'),
  importPack: (path, name) => invoke('import_modpack_file', { path, name }),
  readLog: (id) => invoke('read_log', { id }),
  mods: (id) => invoke('list_mods', { id }),
  toggleMod: (id, name) => invoke('toggle_mod', { id, name }),
  deleteMod: (id, name) => invoke('delete_mod', { id, name }),
  checkModUpdates: (id) => invoke('check_mod_updates', { id }),
  updateMod: (id, currentName, projectId) => invoke('update_mod', { id, currentName, projectId }),
  worlds: (id) => invoke('list_worlds', { id }),
  renameWorld: (id, name, newName) => invoke('rename_world', { id, name, newName }),
  backupWorld: (id, name) => invoke('backup_world', { id, name }),
  openWorld: (id, name) => invoke('open_world', { id, name }),
  searchMods: (id, query) => invoke('search_modrinth_mods', { id, query }),
  installMod: (id, projectId) => invoke('install_modrinth_mod', { id, projectId }),
  searchResourcePacks: (id, query) => invoke('search_modrinth_resourcepacks', { id, query }),
  installResourcePack: (id, projectId) => invoke('install_modrinth_resourcepack', { id, projectId }),
  resourcePacks: (id) => invoke('list_resource_packs', { id }),
  deleteResourcePack: (id, name) => invoke('delete_resource_pack', { id, name }),
};

/** Subscribe to a backend event; the callback gets the payload. */
export const on = (event, cb) => listen(event, (e) => cb(e.payload));
