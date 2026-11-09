import { create } from 'zustand';

import {
    BaseDirectory,
    mkdir,
    readTextFile,
    writeTextFile
} from '@/platform/cacheFiles';
import { isRecord } from '@/shared/utils/record';

const CACHE_FILE = 'navigation-state.json';
let hydration: Promise<void> | undefined;
let writeQueue = Promise.resolve();

type NavigationCacheStore = {
    lastRoute: string;
    folders: Record<string, boolean>;
    settingsCards: Record<string, boolean>;
    toolRows: Record<string, boolean>;
    hydrated: boolean;
    hydrate(): Promise<void>;
    setLastRoute(route: string): void;
    setFolderOpen(index: string, open: boolean): void;
    setSettingsCardOpen(id: string, open: boolean): void;
    setToolRowOpen(toolKey: string, open: boolean): void;
};

function readExpansionStates(value: unknown): Record<string, boolean> {
    if (!isRecord(value)) return {};
    return Object.fromEntries(
        Object.entries(value).filter(
            (entry): entry is [string, boolean] => typeof entry[1] === 'boolean'
        )
    );
}

function persistNavigation(): void {
    const { lastRoute, folders, settingsCards, toolRows } =
        useNavigationCacheStore.getState();
    const contents = JSON.stringify({
        lastRoute,
        folders,
        settingsCards,
        toolRows
    });
    writeQueue = writeQueue
        .then(async () => {
            await mkdir('', {
                baseDir: BaseDirectory.AppCache,
                recursive: true
            });
            await writeTextFile(CACHE_FILE, contents, {
                baseDir: BaseDirectory.AppCache
            });
        })
        .catch((error: unknown) => {
            console.warn('Failed to save navigation cache:', error);
        });
}

export const useNavigationCacheStore = create<NavigationCacheStore>(
    (set, get) => ({
        lastRoute: '/feed',
        folders: {},
        settingsCards: {},
        toolRows: {},
        hydrated: false,
        hydrate: () => {
            hydration ??= (async () => {
                try {
                    const value: unknown = JSON.parse(
                        await readTextFile(CACHE_FILE, {
                            baseDir: BaseDirectory.AppCache
                        })
                    );
                    if (isRecord(value)) {
                        set({
                            lastRoute:
                                typeof value.lastRoute === 'string'
                                    ? value.lastRoute
                                    : '/feed',
                            folders: readExpansionStates(value.folders),
                            settingsCards: readExpansionStates(
                                value.settingsCards
                            ),
                            toolRows: readExpansionStates(value.toolRows)
                        });
                    }
                } catch {
                    // Missing or damaged cache must not prevent startup.
                } finally {
                    set({ hydrated: true });
                }
            })();
            return hydration;
        },
        setLastRoute: (lastRoute) => {
            if (!get().hydrated || get().lastRoute === lastRoute) return;
            set({ lastRoute });
            persistNavigation();
        },
        setFolderOpen: (index, open) => {
            if (!get().hydrated || get().folders[index] === open) return;
            set({ folders: { ...get().folders, [index]: open } });
            persistNavigation();
        },
        setSettingsCardOpen: (id, open) => {
            if (!get().hydrated || get().settingsCards[id] === open) return;
            set({ settingsCards: { ...get().settingsCards, [id]: open } });
            persistNavigation();
        },
        setToolRowOpen: (toolKey, open) => {
            if (!get().hydrated || get().toolRows[toolKey] === open) return;
            set({ toolRows: { ...get().toolRows, [toolKey]: open } });
            persistNavigation();
        }
    })
);
