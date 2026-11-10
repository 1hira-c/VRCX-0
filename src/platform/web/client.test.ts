// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';

import { invokeWeb, reloadAfterWebRestart, webRequest } from './client';
import { webUrl } from './environment';

afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.useRealTimers();
    document.querySelector('base')?.remove();
});

describe('Web transport', () => {
    it('waits through shutdown until a new server instance invalidates the old session', async () => {
        vi.useFakeTimers();
        const reload = vi.fn();
        vi.stubGlobal('window', { location: { reload } });
        const fetch = vi
            .fn()
            .mockResolvedValueOnce(new Response('{}', { status: 200 }))
            .mockRejectedValueOnce(new TypeError('connection closed'))
            .mockResolvedValueOnce(new Response('{}', { status: 401 }));
        vi.stubGlobal('fetch', fetch);
        const restarting = reloadAfterWebRestart();
        await vi.advanceTimersByTimeAsync(100);
        expect(reload).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(100);
        await restarting;
        expect(reload).toHaveBeenCalledOnce();
        expect(fetch.mock.calls[0][1]).toMatchObject({
            credentials: 'same-origin',
            cache: 'no-store'
        });
    });
    it('keeps commands and event URLs inside the Home Assistant ingress prefix', () => {
        const base = document.createElement('base');
        base.href = 'https://ha.example/api/hassio_ingress/test-session/';
        document.head.append(base);
        expect(webUrl('api/events').href).toBe(
            'https://ha.example/api/hassio_ingress/test-session/api/events'
        );
        expect(webUrl('api/invoke/storage__get_all').pathname).toBe(
            '/api/hassio_ingress/test-session/api/invoke/storage__get_all'
        );
    });
    it('uses same-origin cookies and a non-simple CSRF header for commands', async () => {
        const fetch = vi
            .fn()
            .mockResolvedValue(new Response(JSON.stringify({ ok: true })));
        vi.stubGlobal('fetch', fetch);
        await expect(
            invokeWeb('storage__set', { key: 'x', value: 'y' })
        ).resolves.toEqual({ ok: true });
        expect(fetch.mock.calls[0][1]).toMatchObject({
            credentials: 'same-origin',
            method: 'POST',
            headers: { 'X-VRCX-Web': '1', 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: 'x', value: 'y' })
        });
    });
    it('preserves structured backend errors including VRChat status', async () => {
        const error = {
            code: 'vrchat_api',
            message: 'Not found',
            statusCode: 404
        };
        vi.stubGlobal(
            'fetch',
            vi
                .fn()
                .mockResolvedValue(
                    new Response(JSON.stringify(error), { status: 400 })
                )
        );
        await expect(invokeWeb('app__vrchat_user_get')).rejects.toEqual(error);
    });
    it('does not put credentials into session-check URLs', async () => {
        const fetch = vi.fn().mockResolvedValue(new Response('{}'));
        vi.stubGlobal('fetch', fetch);
        await webRequest('api/session');
        expect(fetch.mock.calls[0][1]).toMatchObject({
            method: 'GET',
            body: undefined
        });
    });
    it('opens external links on the browser device, and rejects executable schemes', async () => {
        const open = vi.spyOn(window, 'open').mockReturnValue(null);
        await invokeWeb('app__open_link', { url: 'https://vrchat.com/home' });
        expect(open).toHaveBeenCalledWith(
            'https://vrchat.com/home',
            '_blank',
            'noopener,noreferrer'
        );
        await expect(
            invokeWeb('app__open_link', { url: 'javascript:alert(1)' })
        ).rejects.toThrow('Unsupported link');
        await expect(
            invokeWeb('app__open_link', { url: 'file:///etc/passwd' })
        ).rejects.toThrow('Unsupported link');
        expect(open).toHaveBeenCalledTimes(1);
    });
});
