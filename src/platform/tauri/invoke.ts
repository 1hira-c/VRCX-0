import { invoke, type InvokeArgs } from '@tauri-apps/api/core';

import { invokeWeb } from '../web/client';
import { isWeb } from '../web/environment';

export async function invokeTauri<TReturn = unknown>(
    command: string,
    args?: InvokeArgs
): Promise<TReturn> {
    if (isWeb)
        return invokeWeb<TReturn>(
            command,
            args as Record<string, unknown> | undefined
        );
    return invoke<TReturn>(command, args);
}
