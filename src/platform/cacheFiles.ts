import * as fs from '@tauri-apps/plugin-fs';

import { isWeb } from './web/environment';
export const BaseDirectory = fs.BaseDirectory;
export const mkdir: typeof fs.mkdir = async (...args) => {
    if (!isWeb) return fs.mkdir(...args);
};
export const readTextFile: typeof fs.readTextFile = async (path, options) => {
    if (!isWeb) return fs.readTextFile(path, options);
    return localStorage.getItem(`vrcx0-cache:${String(path)}`) ?? '';
};
export const writeTextFile: typeof fs.writeTextFile = async (
    path,
    contents,
    options
) => {
    if (!isWeb) return fs.writeTextFile(path, contents, options);
    if (typeof contents !== 'string')
        throw new Error('Unsupported cache contents');
    localStorage.setItem(`vrcx0-cache:${String(path)}`, contents);
};
