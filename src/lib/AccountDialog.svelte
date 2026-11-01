<script>
  import Modal from './Modal.svelte';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import { app, login, cancelLogin, logout, selectAccount, saveSettings } from './state.svelte.js';

  let name = $state(app.settings?.username ?? 'Player');
  let { startAdding = false } = $props();
  let adding = $state(false);
  let initialized = false;
  let provider = $state('microsoft');
  let email = $state('');
  let password = $state('');
  let totp = $state('');
  const providers = [
    { id: 'microsoft', name: 'Microsoft', detail: 'Official account', mark: 'M' },
    { id: 'elyby', name: 'Ely.by', detail: 'Alternative account', mark: 'E' },
    { id: 'littleskin', name: 'LittleSkin', detail: 'Alternative account', mark: 'L' },
  ];
  const providerName = (key) => providers.find((p) => p.id === key)?.name ?? key;

  function signIn() {
    const selectedProvider = provider;
    const selectedEmail = email;
    const selectedPassword = password;
    const selectedTotp = totp;
    password = '';
    totp = '';
    login(selectedProvider, selectedEmail, selectedPassword, selectedTotp);
  }

  function saveName() {
    name = name.replace(/[^A-Za-z0-9_]/g, '').slice(0, 16) || 'Player';
    saveSettings({ username: name });
  }

  $effect(() => {
    if (initialized) return;
    adding = startAdding || app.accounts.length === 0;
    initialized = true;
  });
  $effect(() => { if (app.login.status === 'done') adding = false; });
</script>

<Modal title="Your accounts" wide onclose={() => (app.dialog = null)}>
  <div class="intro">
    <div class="intro-copy">
      <span class="eyebrow">SUNRISE PROFILE</span>
      <h2>Pick your player</h2>
      <p>Choose an account for your next launch, or add another one to keep your worlds and identities close.</p>
    </div>
    <div class="intro-art" aria-hidden="true"><span></span><i></i><b></b></div>
  </div>

  {#if app.accounts.length}
    <section class="saved-section">
      <div class="section-heading"><h3>Saved accounts</h3><span>{app.accounts.length}</span></div>
      <div class="accounts">
        {#each app.accounts as account (account.id)}
          <div class="account" class:active={account.id === app.activeAccountId}>
            <Avatar size={44} skinUrl={account.skinUrl} />
            <div class="details">
              <b>{account.username}</b>
              <small>{providerName(account.provider)}</small>
            </div>
            {#if account.id === app.activeAccountId}
              <span class="active-pill"><Icon name="check" size={14} /> Active</span>
            {:else}
              <button class="btn sm" onclick={() => selectAccount(account.id)}>Use account</button>
            {/if}
            <button class="icon-btn remove" title="Remove account" aria-label="Remove {account.username}" onclick={() => logout(account.id)}><Icon name="close" size={16} /></button>
          </div>
        {/each}
      </div>
    </section>
  {:else}
    <div class="empty-state"><span class="empty-mark"><Icon name="user" size={22} /></span><span><b>No online account yet</b><small>Add one to play on authenticated servers. You can still use offline mode below.</small></span></div>
  {/if}

  {#if !adding && app.login.status !== 'waiting'}
    <button class="add-row" onclick={() => (adding = true)}><span class="add-mark"><Icon name="plus" size={20} /></span><span><b>Add another account</b><small>Microsoft, Ely.by or LittleSkin</small></span><span class="add-arrow"><Icon name="chevron" size={18} /></span></button>
  {/if}

  {#if adding}
    <section class="add-panel">
      <div class="panel-heading">
        <div><span class="eyebrow">{app.accounts.length ? 'ADD A PLAYER' : 'GET STARTED'}</span><h3>Sign in to an account</h3></div>
        {#if app.accounts.length && app.login.status !== 'waiting'}<button class="text-action" onclick={() => (adding = false)}>Done</button>{/if}
      </div>

      {#if app.login.status === 'waiting'}
        <div class="login-state"><span class="spin"></span><div><b>{provider === 'microsoft' ? 'Complete sign-in in your browser' : `Connecting to ${providerName(provider)}`}</b><small>Sunrise will add this account here when it’s ready.</small></div></div>
        <button class="btn" onclick={cancelLogin}>Cancel sign-in</button>
      {:else}
        {#if app.login.status === 'error'}<div class="error-state"><Icon name="info" size={18} /><span>{app.login.message}</span></div>{/if}

        <div class="provider-grid" aria-label="Choose sign-in provider">
          {#each providers as option (option.id)}
            <button class="provider" class:selected={provider === option.id} onclick={() => (provider = option.id)} aria-pressed={provider === option.id}>
              <span class="provider-mark" class:ms={option.id === 'microsoft'} class:ely={option.id === 'elyby'} class:little={option.id === 'littleskin'}>{option.mark}</span>
              <span class="provider-label"><b>{option.name}</b><small>{option.detail}</small></span>
              {#if provider === option.id}<span class="provider-check"><Icon name="check" size={14} /></span>{/if}
            </button>
          {/each}
        </div>

        {#if provider === 'microsoft'}
          {#if !app.settings?.msClientId}
            <div class="info-state"><Icon name="info" size={18} /><span>Add your Azure client ID in <b>Settings → Microsoft sign-in</b> before continuing.</span></div>
          {/if}
          <button class="btn primary submit" disabled={!app.settings?.msClientId} onclick={() => login('microsoft')}><Icon name="user" size={17} /> Continue with Microsoft</button>
        {:else}
          <div class="credentials">
            <div class="field">
              <label for="alt-email">Account name or email</label>
              <input id="alt-email" class="input" autocomplete="username" placeholder="name@example.com" bind:value={email} />
            </div>
            <div class="field">
              <label for="alt-password">Password</label>
              <input id="alt-password" class="input" type="password" autocomplete="current-password" placeholder="Enter your password" bind:value={password} />
            </div>
            {#if provider === 'elyby'}
              <div class="field">
                <label for="alt-totp">Two-factor code <span>(optional)</span></label>
                <input id="alt-totp" class="input" inputmode="numeric" autocomplete="one-time-code" placeholder="123456" bind:value={totp} />
              </div>
            {/if}
          </div>
          <button class="btn primary submit" disabled={!email.trim() || !password} onclick={signIn}><Icon name="user" size={17} /> Continue with {providerName(provider)}</button>
          <p class="privacy-note">Your password is used to sign in and isn’t saved. Sunrise keeps the provider’s access token for future launches.</p>
        {/if}
      {/if}
    </section>
  {/if}

  <section class="offline">
    <span class="offline-mark"><Icon name="user" size={16} /></span>
    <div class="offline-copy"><b>Offline profile</b><small>For single-player or offline-mode servers</small></div>
    <input id="off" aria-label="Offline username" bind:value={name} onchange={saveName} maxlength="16" />
  </section>
</Modal>

<style>
  .intro { display: flex; align-items: center; justify-content: space-between; min-height: 126px; padding: 20px 24px; margin: 0 0 18px; overflow: hidden; position: relative; border-radius: 24px 18px 28px 18px; background: linear-gradient(112deg, var(--primary-container), color-mix(in srgb, var(--tertiary-container) 78%, var(--surface))); }
  .intro-copy { position: relative; z-index: 1; max-width: 440px; }
  .eyebrow { font-size: 10px; font-weight: 800; letter-spacing: 1.2px; color: var(--primary); }
  .intro h2 { font-size: 24px; letter-spacing: -.5px; margin: 4px 0; }
  .intro p { max-width: 430px; font-size: 13px; line-height: 1.45; color: var(--text-dim); }
  .intro-art { width: 110px; height: 100px; position: relative; flex: none; margin-right: 10px; }
  .intro-art span { position: absolute; width: 68px; height: 68px; right: 20px; top: 14px; border-radius: 28px 22px 34px 20px; background: var(--primary); transform: rotate(-12deg); opacity: .9; }
  .intro-art i { position: absolute; width: 35px; height: 35px; right: 0; top: 2px; border-radius: 50%; background: var(--tertiary); border: 5px solid color-mix(in srgb, var(--tertiary-container) 70%, white); }
  .intro-art b { position: absolute; width: 26px; height: 26px; left: 5px; bottom: 5px; border-radius: 9px 50% 50% 50%; background: var(--scene-glow); transform: rotate(25deg); }

  .saved-section { margin: 0 0 14px; }
  .section-heading { display: flex; align-items: center; gap: 8px; margin: 0 2px 8px; }
  .section-heading h3, .panel-heading h3 { font-size: 14px; font-weight: 800; letter-spacing: .1px; }
  .section-heading > span { min-width: 22px; height: 22px; display: grid; place-items: center; padding: 0 6px; border-radius: 9px; background: var(--secondary-container); color: var(--on-secondary-container); font-size: 11px; font-weight: 800; }
  .accounts { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 9px; max-height: 190px; overflow-y: auto; padding: 1px 2px 4px; }
  .account { min-width: 0; display: flex; align-items: center; gap: 10px; padding: 9px; border: 1px solid var(--outline); border-radius: 16px 12px 16px 12px; background: var(--surface); transition: transform .22s var(--spring), border-radius .25s var(--spring), background .2s; }
  .account:hover { transform: translateY(-1px); border-radius: 20px 12px 20px 12px; }
  .account.active { border-color: color-mix(in srgb, var(--primary) 65%, var(--outline)); background: var(--primary-container); color: var(--on-primary-container); }
  .details { display: grid; min-width: 0; flex: 1; }
  .details b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .details small { color: var(--text-dim); font-size: 12px; }
  .active-pill { display: inline-flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: 999px; background: color-mix(in srgb, var(--primary) 16%, transparent); color: var(--on-primary-container); font-size: 11px; font-weight: 800; white-space: nowrap; }
  .icon-btn.remove { width: 30px; height: 30px; border-radius: 50%; }
  .icon-btn.remove:hover { background: color-mix(in srgb, var(--error) 14%, transparent); color: var(--error); }
  .empty-state { display: flex; align-items: center; gap: 12px; margin: 6px 0 14px; padding: 14px; border-radius: 18px; background: var(--surface-high); }
  .empty-mark { width: 42px; height: 42px; display: grid; place-items: center; flex: none; border-radius: 15px 50% 50% 50%; background: var(--secondary-container); color: var(--primary); }
  .empty-state > span:last-child { display: grid; gap: 3px; }
  .empty-state small { color: var(--text-dim); font-size: 12px; line-height: 1.35; }

  .add-row { width: 100%; display: flex; align-items: center; gap: 11px; padding: 11px 13px; margin: 8px 0 14px; text-align: left; border: 1px dashed var(--outline); border-radius: 18px 14px 18px 14px; transition: background .2s, border-color .2s, border-radius .25s var(--spring), transform .2s var(--spring); }
  .add-row:hover { background: var(--secondary-container); border-color: var(--primary); border-radius: 24px 14px 24px 14px; transform: translateY(-1px); }
  .add-mark { width: 36px; height: 36px; display: grid; place-items: center; border-radius: 12px 50% 50% 50%; background: var(--primary-container); color: var(--primary); }
  .add-row > span:nth-child(2) { display: grid; flex: 1; gap: 2px; }
  .add-row small { color: var(--text-dim); font-size: 12px; }
  .add-arrow { color: var(--text-dim); transform: rotate(-90deg); }

  .add-panel { padding: 16px; border-radius: 20px 16px 24px 16px; background: var(--surface-high); margin: 10px 0 14px; }
  .panel-heading { display: flex; justify-content: space-between; align-items: center; margin: 0 2px 12px; }
  .panel-heading h3 { margin-top: 3px; }
  .text-action { color: var(--primary); font-weight: 800; padding: 8px 12px; border-radius: 999px; }
  .text-action:hover { background: color-mix(in srgb, var(--primary) 12%, transparent); }
  .provider-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; margin-bottom: 14px; }
  .provider { min-width: 0; min-height: 68px; display: flex; align-items: center; gap: 9px; position: relative; padding: 10px; text-align: left; border: 1px solid var(--outline); border-radius: 14px 12px 16px 12px; background: var(--surface); transition: transform .2s var(--spring), border-radius .25s var(--spring), border-color .2s, background .2s; }
  .provider:hover { transform: translateY(-2px); border-radius: 19px 12px 20px 12px; }
  .provider.selected { border: 2px solid var(--primary); padding: 9px; border-radius: 18px 12px 20px 12px; background: var(--primary-container); color: var(--on-primary-container); }
  .provider-mark { width: 34px; height: 34px; flex: none; display: grid; place-items: center; border-radius: 12px; color: white; font-weight: 900; font-size: 17px; transition: border-radius .25s var(--spring), transform .25s var(--spring); }
  .provider:hover .provider-mark, .provider.selected .provider-mark { border-radius: 50% 50% 45% 55%; transform: rotate(-8deg); }
  .provider-mark.ms { background: #1877D2; }
  .provider-mark.ely { background: #6D4BB8; }
  .provider-mark.little { background: #E06E8D; }
  .provider-label { min-width: 0; display: grid; gap: 2px; }
  .provider-label b { font-size: 13px; }
  .provider-label small { font-size: 10px; line-height: 1.2; color: var(--text-dim); }
  .provider-check { position: absolute; top: 6px; right: 6px; width: 18px; height: 18px; display: grid; place-items: center; border-radius: 50%; color: var(--on-primary); background: var(--primary); }
  .credentials { display: grid; grid-template-columns: 1fr 1fr; gap: 0 12px; }
  .credentials .field { margin-top: 0; margin-bottom: 12px; }
  .credentials .field:last-child:nth-child(3) { grid-column: 1 / -1; }
  .credentials label { font-size: 12px; font-weight: 800; color: var(--text-dim); }
  .credentials label span { font-weight: 500; }
  .credentials .input { height: 42px; border-radius: 12px; background: var(--surface); }
  .submit { min-height: 42px; height: auto; width: 100%; border-radius: 14px 12px 16px 12px; }
  .submit:hover:not(:disabled) { border-radius: 18px 12px 20px 12px; transform: translateY(-1px); }
  .privacy-note { margin-top: 9px; text-align: center; font-size: 11px; color: var(--text-dim); line-height: 1.4; }
  .info-state, .error-state, .login-state { display: flex; align-items: center; gap: 10px; padding: 11px 12px; border-radius: 14px; margin: 0 0 12px; font-size: 12px; line-height: 1.4; }
  .info-state { background: var(--tertiary-container); color: var(--on-tertiary-container); }
  .error-state { background: color-mix(in srgb, var(--error) 12%, var(--surface)); color: var(--error); }
  .login-state { background: var(--primary-container); color: var(--on-primary-container); margin-bottom: 12px; }
  .login-state > div { display: grid; gap: 2px; }
  .login-state small { color: var(--text-dim); }

  .offline { display: flex; align-items: center; gap: 10px; margin-top: 14px; padding: 11px 12px; border: 1px solid var(--outline); border-radius: 16px; background: var(--surface); }
  .offline-mark { width: 30px; height: 30px; display: grid; place-items: center; flex: none; border-radius: 50%; background: var(--secondary-container); color: var(--text-dim); }
  .offline-copy { display: grid; flex: 1; min-width: 0; }
  .offline-copy b { font-size: 12px; }
  .offline-copy small { font-size: 10px; color: var(--text-dim); }
  .offline input { width: 126px; height: 34px; padding: 0 10px; border: 1px solid var(--outline); border-radius: 10px; background: var(--bg); color: var(--text); }
  .offline input:focus { outline: 2px solid color-mix(in srgb, var(--primary) 55%, transparent); border-color: var(--primary); }

  @media (max-width: 620px) {
    .intro { padding: 17px; }
    .intro-art { transform: scale(.8); transform-origin: center right; margin-right: -10px; }
    .provider-grid { grid-template-columns: 1fr; }
    .provider { min-height: 54px; }
    .credentials { grid-template-columns: 1fr; }
    .credentials .field:last-child:nth-child(3) { grid-column: auto; }
    .offline { flex-wrap: wrap; }
    .offline input { flex: 1; }
  }
</style>
