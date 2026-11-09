// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';

class FakeEventSource {
    static instances: FakeEventSource[] = [];
    onopen: (() => void) | null = null;
    onerror: (() => void) | null = null;
    onmessage: ((event: { data: string }) => void) | null = null;
    close = vi.fn();
    constructor(readonly url: URL) {
        FakeEventSource.instances.push(this);
    }
}
afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
    FakeEventSource.instances = [];
});

it('shares a single SSE connection and waits for subscription before snapshots', async () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    const { listenWeb } = await import('./events');
    const first = vi.fn();
    const second = vi.fn();
    const a = listenWeb('feed', first);
    const b = listenWeb('auth', second);
    expect(FakeEventSource.instances).toHaveLength(1);
    const source = FakeEventSource.instances[0];
    let ready = false;
    void a.then(() => {
        ready = true;
    });
    await Promise.resolve();
    expect(ready).toBe(false);
    source.onopen?.();
    const [offA, offB] = await Promise.all([a, b]);
    source.onmessage?.({
        data: JSON.stringify({ event: 'feed', payload: { revision: 2 } })
    });
    expect(first).toHaveBeenCalledWith({
        event: 'feed',
        id: 0,
        payload: { revision: 2 }
    });
    expect(second).not.toHaveBeenCalled();
    offA();
    expect(source.close).not.toHaveBeenCalled();
    offB();
    expect(source.close).toHaveBeenCalledOnce();
});
