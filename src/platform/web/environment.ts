export const isWeb = import.meta.env.MODE === 'web';

// Hash routing keeps this base stable, including Home Assistant's ingress prefix.
export function webUrl(path: string): URL {
    return new URL(path, new URL('.', document.baseURI));
}
