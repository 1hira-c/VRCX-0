import type { Event, UnlistenFn } from '@tauri-apps/api/event';

import { webRequest } from './client';
import { webUrl } from './environment';

type Handler = (event: Event<unknown>) => void;
const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
let connected: Promise<void> | null = null;

export async function listenWeb<T>(
    name: string,
    handler: (event: Event<T>) => void
): Promise<UnlistenFn> {
    const callback = handler as Handler;
    const bucket = handlers.get(name) ?? new Set<Handler>();
    handlers.set(name, bucket);
    bucket.add(callback);
    if (!source) {
        const stream = new EventSource(webUrl('api/events'));
        source = stream;
        connected = new Promise<void>((resolve) => {
            let opened = false;
            stream.onopen = () => {
                if (opened) window.location.reload(); // Rehydrate snapshots after a connection gap.
                opened = true;
                resolve();
            };
            stream.onerror = () => {
                void webRequest('api/session')
                    .then((response) => {
                        if (response.status === 401) window.location.reload();
                    })
                    .catch(() => {});
            };
        });
        stream.onmessage = (message) => {
            const value = JSON.parse(String(message.data)) as {
                event: string;
                payload: unknown;
            };
            if (value.event === 'web:resync') {
                window.location.reload();
                return;
            }
            for (const listener of handlers.get(value.event) ?? []) {
                try {
                    listener({
                        event: value.event,
                        id: 0,
                        payload: value.payload
                    });
                } catch (error) {
                    console.error('Web event handler failed:', error);
                }
            }
        };
    }
    await connected;
    return () => {
        bucket.delete(callback);
        if (!bucket.size) handlers.delete(name);
        if (!handlers.size) {
            source?.close();
            source = null;
            connected = null;
        }
    };
}
