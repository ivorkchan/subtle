console.info('Basic loading');

import { path } from "@tauri-apps/api";
import * as os from "@tauri-apps/plugin-os";
import * as fs from "@tauri-apps/plugin-fs";
import { getVersion } from "@tauri-apps/api/app";

let version = '?';
getVersion().then((x) => version = x);

const osType = os.type(),
      pathSeparator = path.sep(),
      ctrlKey = os.type() == 'macos' ? 'Meta' : 'Control';

export const Basic = {
    get version() { return version; },
    get osType() { return osType; },
    get pathSeparator() { return pathSeparator; },
    get ctrlKey() { return ctrlKey; },

    approx(a: number, b: number, d = 0.0001) {
        return Math.abs(a - b) < d;
    },

    getFilename(p: string) { 
        return p.split(Basic.pathSeparator).at(-1);
    },

    escapeRegexp(str: string) {
        return str.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&');
    },

    splitPrintingWords(str: string): string[] {
        return str.split(/(\n)/u).flatMap((x) => x.split(
            /(?<=[^\u4E00-\u9FFF])(?=[\u4E00-\u9FFF])|(?<= )|(?<=[\u4E00-\u9FFF])(?=[\u0000-\u00FF\u4E00-\u9FFF])/u));
    },

    timeout<T>(t: number, p: Promise<T>): Promise<T> {
        return Promise.race([p, 
            new Promise<T>((_, reject) => setTimeout(() => reject('timeout'), t))]);
    },

    /**
     * Resolves in `n` milliseconds using `setTimeout`.
     */
    wait(n: number) {
        return new Promise<void>((resolve) => setTimeout(() => resolve(), n));
    },

    parseTimestamp: (t: string) => {
        const reg = /(\d+):(\d+):(\d+)[,.](\d+)/;
        let match = reg.exec(t);
        if (!match) return null;
        let h = parseInt(match[1]),
            m = parseInt(match[2]),
            s = parseFloat(match[3] + '.' + match[4]);
        let result = h * 3600 + m * 60 + s;
        if (isNaN(result)) return null;
        return result;
    },

    formatTimestamp: (t: number, n: number = 3, char = '.') => {
        let h = Math.floor(t / 3600).toString().padStart(2, '0');
        let m = Math.floor((t % 3600) / 60).toString().padStart(2, '0');
        let s = Math.floor(t % 60).toString().padStart(2, '0');
        let ms = (t % 1).toFixed(n).slice(2);
        return `${h}:${m}:${s}${char}${ms}`;
    },
    
    normalizeNewlines: (s: string) => {
        return s.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    },

    async ensureConfigDirectoryExists() {
        const configDir = await path.appConfigDir();
        if (!await fs.exists(configDir))
            await fs.mkdir(configDir, {recursive: true});
    }
}