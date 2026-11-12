import {
  ConnectionState,
  Room,
  RoomEvent,
  Track,
  type RemoteParticipant,
  type RemoteTrackPublication,
  type RemoteVideoTrack,
} from 'livekit-client';
import type { VideoSession, VideoStats, VideoUpdate, ViewerCredentials } from './types';

const STATS_INTERVAL_MS = 1000;
/** Sem vídeo depois disso, o visualizador avisa em vez de ficar "aguardando" para sempre. */
const WAIT_WARNING_MS = 10_000;

function screenPublication(p: RemoteParticipant | undefined): RemoteTrackPublication | undefined {
  return p?.getTrackPublication(Track.Source.ScreenShare) as RemoteTrackPublication | undefined;
}

/**
 * Conecta à sala como visualizador (somente assinatura) e liga a tela de cada participante a um
 * `<video>`: a decodificação H.264/VP8 fica com a WebView (aceleração de hardware) e não há
 * conversão para imagem nem passagem de quadros pelo IPC do Tauri.
 */
export async function connectVideoSession({ url, token }: ViewerCredentials): Promise<VideoSession> {
  const room = new Room({ adaptiveStream: false, dynacast: false });
  // Áudio fica com o cliente nativo; aqui só assinamos as telas.
  await room.connect(url, token, { autoSubscribe: false });

  const subscribeScreens = (p: RemoteParticipant) => {
    for (const pub of p.trackPublications.values()) {
      if (pub.source === Track.Source.ScreenShare && !pub.isSubscribed) pub.setSubscribed(true);
    }
  };
  room.remoteParticipants.forEach(subscribeScreens);
  room.on(RoomEvent.ParticipantConnected, subscribeScreens);
  room.on(RoomEvent.TrackPublished, (pub, p) => {
    if (pub.source === Track.Source.ScreenShare) pub.setSubscribed(true);
    void p;
  });

  return {
    watch(participantId, el, onUpdate) {
      let track: RemoteVideoTrack | undefined;
      let prev: { bytes: number; frames: number; at: number } | undefined;
      let waitingSince = Date.now();
      let last: VideoUpdate = { status: 'connecting' };

      const emit = (u: VideoUpdate) => {
        last = u;
        onUpdate(u);
      };

      const detach = () => {
        track?.detach(el);
        track = undefined;
        prev = undefined;
      };

      const bind = () => {
        const next = screenPublication(room.remoteParticipants.get(participantId))?.videoTrack as
          | RemoteVideoTrack
          | undefined;
        if (next === track) return;
        detach();
        track = next;
        if (track) {
          track.attach(el);
          void el.play().catch(() => {});
        } else {
          waitingSince = Date.now();
          emit({ status: 'waiting' });
        }
      };

      const poll = async () => {
        if (room.state !== ConnectionState.Connected) {
          emit({ status: 'connecting', message: 'Reconectando ao vídeo…' });
          return;
        }
        if (!track) {
          if (Date.now() - waitingSince > WAIT_WARNING_MS) {
            emit({ status: 'waiting', message: 'A imagem ainda não chegou. Verifique a conexão com o servidor.' });
          }
          return;
        }
        const s = (await track.getReceiverStats()) as
          | { bytesReceived?: number; framesDecoded?: number; frameWidth?: number; frameHeight?: number }
          | undefined;
        if (!s) return;
        const now = performance.now();
        const bytes = s.bytesReceived ?? 0;
        const frames = s.framesDecoded ?? 0;
        let stats: VideoStats | undefined;
        if (prev && now > prev.at) {
          const secs = (now - prev.at) / 1000;
          stats = {
            width: s.frameWidth ?? el.videoWidth,
            height: s.frameHeight ?? el.videoHeight,
            fps: Math.round((frames - prev.frames) / secs),
            bitrateKbps: Math.round(((bytes - prev.bytes) * 8) / 1000 / secs),
          };
        }
        prev = { bytes, frames, at: now };
        if (frames > 0) emit({ status: 'playing', stats: stats ?? last.stats });
      };

      room.on(RoomEvent.TrackSubscribed, bind);
      room.on(RoomEvent.TrackUnsubscribed, bind);
      room.on(RoomEvent.TrackPublished, bind);
      room.on(RoomEvent.TrackUnpublished, bind);
      room.on(RoomEvent.ParticipantConnected, bind);
      room.on(RoomEvent.ParticipantDisconnected, bind);
      room.on(RoomEvent.Reconnecting, () => emit({ status: 'connecting', message: 'Reconectando ao vídeo…' }));
      bind();
      emit(track ? { status: 'connecting' } : { status: 'waiting' });
      const timer = setInterval(() => void poll().catch(() => {}), STATS_INTERVAL_MS);

      return () => {
        clearInterval(timer);
        room.off(RoomEvent.TrackSubscribed, bind);
        room.off(RoomEvent.TrackUnsubscribed, bind);
        room.off(RoomEvent.TrackPublished, bind);
        room.off(RoomEvent.TrackUnpublished, bind);
        room.off(RoomEvent.ParticipantConnected, bind);
        room.off(RoomEvent.ParticipantDisconnected, bind);
        detach();
      };
    },
    close() {
      void room.disconnect();
    },
  };
}
