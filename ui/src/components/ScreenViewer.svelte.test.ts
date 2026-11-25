import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
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
  it('não religa o vídeo quando o sharer é recriado com o mesmo id', async () => {
    const { backend, watch, unwatch } = fakeBackend();
    // Props reativas como no Palco (store.participants), sem o rerender que troca todas as props.
    const props = $state({ backend, sharer: sharer(), pequena: false, onroda: () => {} });
    render(ScreenViewer, { props });
    await waitFor(() => expect(watch).toHaveBeenCalledTimes(1));

    // onParticipants recria o participante inteiro a cada fala ou mute.
    props.sharer = sharer({ speaking: true });
    flushSync();
    props.sharer = sharer({ muted: true });
    flushSync();
    expect(watch).toHaveBeenCalledTimes(1);
    expect(unwatch).not.toHaveBeenCalled();

    props.sharer = sharer({ id: 'caio', name: 'Caio' });
    flushSync();
    expect(watch).toHaveBeenCalledTimes(2);
    expect(watch.mock.calls[1][0]).toBe('caio');
    expect(unwatch).toHaveBeenCalledTimes(1);
  });

  it('avisa quando a imagem trava', async () => {
    const { backend, watch, update } = fakeBackend();
    render(ScreenViewer, { backend, sharer: sharer(), pequena: false, onroda: () => {} });
    await waitFor(() => expect(watch).toHaveBeenCalled());

    update({ status: 'stalled', message: 'A imagem travou. Aguardando um novo quadro…' });
    flushSync();
    expect(screen.getByRole('status')).toHaveTextContent('A imagem travou');
    expect(screen.getByTestId('screen-video')).toHaveClass('visivel');
  });

  it('botão Roda chama onroda e some na tela pequena', async () => {
    const { backend } = fakeBackend();
    const user = userEvent.setup();
    const onroda = vi.fn();
    const { rerender } = render(ScreenViewer, { backend, sharer: sharer(), pequena: false, onroda });

    await user.click(screen.getByRole('button', { name: 'Roda' }));
    expect(onroda).toHaveBeenCalled();

    await rerender({ backend, sharer: sharer(), pequena: true, onroda });
    expect(screen.queryByRole('button', { name: 'Roda' })).toBeNull();
  });
});
