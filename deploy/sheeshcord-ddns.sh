#!/usr/bin/env bash
# Atualiza registros A na Hostinger quando o IP público muda e reinicia o LiveKit, que só
# descobre o IP externo (use_external_ip) ao iniciar: com IP velho, quem está fora da rede
# fica em "wait_pc_connection timed out".
set -euo pipefail

DIR=${SHEESHCORD_DIR:-/opt/sheeshcord}
ENV_FILE=$DIR/.env
STATE=${XDG_STATE_HOME:-$HOME/.local/state}/sheeshcord/livekit-ip
getv() { grep "^$1=" "$ENV_FILE" | cut -d= -f2- | tr -d '\r"'"'"' '; }

DOMAIN_FULL=$(getv DOMAIN)
TOKEN=$(getv HOSTINGER_API_TOKEN)
ZONE=${DOMAIN_FULL#*.}
NAMES=(${DOMAIN_FULL%%.*} paper)
API=https://developers.hostinger.com/api/dns/v1/zones/$ZONE

IP=$(curl -4 -fsS -m 10 https://ifconfig.me || curl -4 -fsS -m 10 https://api.ipify.org)
[[ $IP =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "IP inválido: $IP" >&2; exit 1; }

RECORDS=$(curl -fsS -m 15 -H "Authorization: Bearer $TOKEN" "$API")

for NAME in "${NAMES[@]}"; do
  CURRENT=$(jq -r --arg n "$NAME" '.[] | select(.name==$n and .type=="A") | .records[0].content' <<<"$RECORDS")
  [[ $CURRENT == "$IP" ]] && continue
  BODY=$(jq -n --arg n "$NAME" --arg ip "$IP" \
    '{overwrite:true, zone:[{name:$n, type:"A", ttl:300, records:[{content:$ip}]}]}')
  curl -fsS -m 15 -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -d "$BODY" "$API" >/dev/null
  echo "$(date -Is) $NAME.$ZONE: ${CURRENT:-<vazio>} -> $IP"
done

if [[ $(cat "$STATE" 2>/dev/null) != "$IP" ]]; then
  (cd "$DIR" && docker compose restart livekit >/dev/null)
  mkdir -p "$(dirname "$STATE")"
  echo "$IP" >"$STATE"
  echo "$(date -Is) livekit reiniciado para anunciar $IP"
fi
