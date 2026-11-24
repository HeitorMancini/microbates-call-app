import { render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Backend, Participant, VideoSession, VideoUpdate } from '../lib/types';
import ScreenViewer from './ScreenViewer.svelte';

const sharer = (over: Partial<Participant> = {}): Participant => ({
  id: 'ana',
  name: 'Ana',
  muted: false,
  speaking: false,
  sharingScreen: true,
  isLocal: false,
  ...over,
});

function fakeBackend() {
  let push: (u: VideoUpdate) => void = () => {};
  const unwatch = vi.fn();
  const watch = vi.fn<VideoSession['watch']>((_id, _el, onUpdate) => {
    push = onUpdate;
    return unwatch;
  });
  const session: VideoSession = { watch, close: vi.fn() };
  const backend = { openVideo: vi.fn(async () => session) } as unknown as Backend;
  return { backend, watch, unwatch, update: (u: VideoUpdate) => push(u) };
}

describe('ScreenViewer', () => {
  it('não religa o vídeo quando a lista é recriada com o mesmo compartilhador', async () => {
    const { backend, watch, unwatch } = fakeBackend();
    // Props reativas como no app (store.sharers), sem o rerender que troca todas as props.
    const props = $state({ backend, sharers: [sharer()] });
    render(ScreenViewer, { props });
    await waitFor(() => expect(watch).toHaveBeenCalledTimes(1));

    // onParticipants recria a lista inteira a cada fala ou mute.
    props.sharers = [sharer({ speaking: true })];
    flushSync();
    props.sharers = [sharer({ muted: true })];
    flushSync();
    expect(watch).toHaveBeenCalledTimes(1);
    expect(unwatch).not.toHaveBeenCalled();

    props.sharers = [sharer({ id: 'caio', name: 'Caio' })];
    flushSync();
    expect(watch).toHaveBeenCalledTimes(2);
    expect(watch.mock.calls[1][0]).toBe('caio');
    expect(unwatch).toHaveBeenCalledTimes(1);
  });

  it('avisa quando a imagem trava', async () => {
    const { backend, watch, update } = fakeBackend();
    render(ScreenViewer, { backend, sharers: [sharer()] });
    await waitFor(() => expect(watch).toHaveBeenCalled());

    update({ status: 'stalled', message: 'A imagem travou. Aguardando um novo quadro…' });
    flushSync();
    expect(screen.getByRole('status')).toHaveTextContent('A imagem travou');
    expect(screen.getByTestId('screen-video')).toHaveClass('visivel');
  });
});
