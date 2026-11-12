import type { ScreenFrame } from './types';

export function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/**
 * Desenha frames JPEG num canvas. Se um frame chega enquanto o anterior ainda
 * decodifica, guarda só o mais recente (nunca acumula fila).
 */
export class FrameRenderer {
  #canvas: HTMLCanvasElement;
  #busy = false;
  #pending: ScreenFrame | null = null;
  #raf = 0;
  onFirstFrame?: () => void;
  framesDrawn = 0;

  constructor(canvas: HTMLCanvasElement) {
    this.#canvas = canvas;
  }

  push(frame: ScreenFrame) {
    this.#pending = frame;
    if (!this.#busy) void this.#drain();
  }

  async #drain() {
    this.#busy = true;
    while (this.#pending) {
      const frame = this.#pending;
      this.#pending = null;
      try {
        const bitmap = await createImageBitmap(new Blob([base64ToBytes(frame.jpegBase64)], { type: 'image/jpeg' }));
        await new Promise<void>((resolve) => {
          this.#raf = requestAnimationFrame(() => {
            this.#paint(bitmap, frame);
            resolve();
          });
        });
      } catch {
        // Frame corrompido: pula e segue para o próximo.
      }
    }
    this.#busy = false;
  }

  #paint(bitmap: ImageBitmap, frame: ScreenFrame) {
    const canvas = this.#canvas;
    if (canvas.width !== frame.width || canvas.height !== frame.height) {
      canvas.width = frame.width;
      canvas.height = frame.height;
    }
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, frame.width, frame.height);
    bitmap.close();
    if (this.framesDrawn++ === 0) this.onFirstFrame?.();
  }

  dispose() {
    cancelAnimationFrame(this.#raf);
    this.#pending = null;
  }
}
