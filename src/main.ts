import { createElement, StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import '@/styles/globals.css';
import { installDevPerformanceTimelineGuard } from '@/app/devPerformanceTimelineGuard';
import { isWeb } from '@/platform/web/environment';
import { installErrorLogging } from '@/services/errorLogService';

// only use in dev to prevent OOM from React dev tools User Timing measures
installDevPerformanceTimelineGuard();
installErrorLogging();

async function bootstrap() {
    if (isWeb) {
        await import('@/platform/web/web.css');
        const { authenticateWeb } = await import('@/platform/web/auth');
        await authenticateWeb();
    }
    const [, { App }] = await Promise.all([
        import('@/services/i18nService'),
        import('./app/App')
    ]);

    const rootElement = document.getElementById('root');

    if (!rootElement) {
        throw new Error('Missing #root mount node');
    }

    createRoot(rootElement).render(
        createElement(StrictMode, null, createElement(App))
    );
}

bootstrap().catch((error: unknown) => {
    console.error(error);
    if (isWeb) {
        const root = document.getElementById('root');
        if (root)
            root.textContent = `VRCX-0 Web: ${error instanceof Error ? error.message : String(error)} — reload to retry.`;
    }
});
