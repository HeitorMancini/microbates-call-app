/** Endpoint `POST /token` a partir da URL do servidor (mesma regra de `token_endpoint` no Rust). */
export function tokenEndpoint(serverUrl: string): string {
  const url = serverUrl.trim().replace(/\/+$/, '');
  const base = /^wss?:\/\//.test(url)
    ? url.replace(/^wss:\/\//, 'https://').replace(/^ws:\/\//, 'http://')
    : /^https?:\/\//.test(url)
      ? url
      : `http://${url}`;
  return `${base}/token`;
}
