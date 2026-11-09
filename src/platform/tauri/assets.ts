import { convertFileSrc as tauriConvertFileSrc } from '@tauri-apps/api/core';

import { isWeb } from '../web/environment';

export function convertFileSrc(filePath: string, protocol = 'asset'): string {
    if (isWeb) {
        throw new Error('Local files require a connected desktop agent.');
    }
    return tauriConvertFileSrc(filePath, protocol);
}
