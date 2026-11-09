import { webRequest } from './client';
import { webUrl } from './environment';

// Runs before importing App: no runtime commands are sent until Web authentication succeeds.
export async function authenticateWeb(): Promise<void> {
    const status = await webRequest('api/session');
    if (status.ok) return;
    if (status.status !== 401)
        throw new Error(`Web server unavailable: HTTP ${status.status}`);
    const root = document.getElementById('root');
    if (!root) throw new Error('Missing root');
    const panel = document.createElement('main');
    panel.className = 'web-login';
    panel.innerHTML = `<form><h1>VRCX-0 Web</h1><p>サーバーのアクセストークンで接続 / Connect with your server access token.</p><label for="web-token">Access token</label><input id="web-token" name="token" type="password" autocomplete="current-password" required minlength="32" /><button type="submit">接続 / Connect</button><p role="status"></p><small>初回起動ログ、またはデータディレクトリの web-access-token を確認してください。</small></form>`;
    root.replaceChildren(panel);
    const form = panel.querySelector('form')!;
    const input = panel.querySelector('input')!;
    const button = panel.querySelector('button')!;
    const message = panel.querySelector('[role="status"]')!;
    await new Promise<void>((resolve) => {
        form.addEventListener('submit', (event) => {
            event.preventDefault();
            button.disabled = true;
            void webRequest('api/login', {
                token: input.value,
                cookiePath: webUrl('').pathname
            })
                .then((response) => {
                    if (!response.ok)
                        throw new Error(
                            '接続できません。トークンを確認してください / Check your access token.'
                        );
                    input.value = '';
                    root.replaceChildren();
                    resolve();
                })
                .catch((error: unknown) => {
                    message.textContent =
                        error instanceof Error
                            ? error.message
                            : 'Connection failed';
                })
                .finally(() => {
                    button.disabled = false;
                });
        });
    });
}
