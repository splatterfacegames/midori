/**
 * Host file adapters.
 *
 * Browser hosting saves through anchor downloads and opens through a file
 * input. Tauri hosting uses the dialog plugin for paths and two bounded
 * commands (`read_text_file`, `save_export`) on the native side.
 */

import type { ExportFile } from './model';

export interface FileHost {
  /** Save `files` starting from a user-picked path. Returns a display label, or null when cancelled. */
  saveFiles(defaultName: string, files: ExportFile[]): Promise<string | null>;
  /** Prompt for a TOML species file. Returns its name and text, or null when cancelled. */
  openTextFile(title: string, extensions: string[]): Promise<{ name: string; text: string } | null>;
}

export function createBrowserFileHost(): FileHost {
  return {
    async saveFiles(_defaultName, files) {
      for (const file of files) {
        const url = URL.createObjectURL(new Blob([file.data as BlobPart], { type: 'model/gltf-binary' }));
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = file.name;
        anchor.click();
        URL.revokeObjectURL(url);
      }
      return 'browser download';
    },
    async openTextFile(_title, extensions) {
      return new Promise((resolve) => {
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = extensions.map((ext) => `.${ext}`).join(',');
        input.onchange = async () => {
          const file = input.files?.[0];
          if (!file) return resolve(null);
          resolve({ name: file.name, text: await file.text() });
        };
        input.click();
      });
    },
  };
}

interface TauriDeps {
  invoke: <T>(command: string, args?: import('@tauri-apps/api/core').InvokeArgs) => Promise<T>;
  save: (options: {
    defaultPath?: string;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | null>;
  open: (options: {
    title?: string;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | null>;
}

export function createTauriFileHost({ invoke, save, open }: TauriDeps): FileHost {
  return {
    async saveFiles(defaultName, files) {
      const path = await save({
        defaultPath: defaultName,
        filters: [
          { name: 'glTF binary', extensions: ['glb'] },
          { name: 'glTF document', extensions: ['gltf'] },
        ],
      });
      if (!path) return null;
      const directory = path.slice(0, Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/')) + 1);
      for (const file of files) {
        // Frame: [u32 le path length][path utf8][payload]
        const target = directory + file.name;
        const pathBytes = new TextEncoder().encode(target);
        const frame = new Uint8Array(4 + pathBytes.length + file.data.length);
        new DataView(frame.buffer).setUint32(0, pathBytes.length, true);
        frame.set(pathBytes, 4);
        frame.set(file.data, 4 + pathBytes.length);
        await invoke('save_export', frame);
      }
      return directory || path;
    },
    async openTextFile(title, extensions) {
      const path = await open({ title, filters: [{ name: 'Species TOML', extensions }] });
      if (!path) return null;
      const text = await invoke<string>('read_text_file', { path });
      const name = path.split(/[\\/]/).pop() ?? path;
      return { name, text };
    },
  };
}
