import { webUrl } from './environment';

export async function webRequest(
    path: string,
    body?: unknown
): Promise<Response> {
    return fetch(webUrl(path), {
        method: body === undefined ? 'GET' : 'POST',
        credentials: 'same-origin',
        headers:
            body === undefined
                ? {}
                : {
                      'Content-Type': 'application/json',
                      'X-VRCX-Web': '1'
                  },
        body: body === undefined ? undefined : JSON.stringify(body)
    });
}

export async function reloadAfterWebRestart(): Promise<void> {
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
        try {
            // Sessions belong to one server instance. Wait for the replacement instance
            // before reloading so a successful recovery cannot land on a closed listener.
            const response = await fetch(webUrl('api/session'), {
                credentials: 'same-origin',
                cache: 'no-store',
                signal: AbortSignal.timeout(1000)
            });
            if (response.status === 401) {
                window.location.reload();
                return;
            }
        } catch {
            // A short connection failure is expected while the server restarts.
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error(
        'The server is still restarting. Reload this page to reconnect.'
    );
}

export async function invokeWeb<T>(
    command: string,
    args: Record<string, unknown> = {}
): Promise<T> {
    if (
        command === 'app__open_link' ||
        command === 'app__open_discord_profile'
    ) {
        const url = new URL(
            command === 'app__open_link'
                ? String(args.url)
                : `https://discord.com/users/${encodeURIComponent(String(args.discordId))}`
        );
        if (!['http:', 'https:'].includes(url.protocol))
            throw new Error('Unsupported link protocol');
        window.open(url.href, '_blank', 'noopener,noreferrer');
        return undefined as T;
    }
    if (
        [
            'app__set_tray_icon_notification',
            'app__set_taskbar_overlay_notification',
            'app__refresh_tray_menu',
            'app__telemetry_record_event'
        ].includes(command)
    )
        return undefined as T;
    if (command === 'app__get_clipboard')
        return (await navigator.clipboard.readText()) as T;
    const response = await webRequest(
        `api/invoke/${encodeURIComponent(command)}`,
        args
    );
    if (response.status === 401) {
        window.location.reload();
        throw new Error('Web session expired. Sign in again.');
    }
    const value: unknown = await response
        .json()
        .catch(() => ({ message: `HTTP ${response.status}` }));
    if (!response.ok) throw value;
    return value as T;
}
