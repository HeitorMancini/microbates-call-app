#!/usr/bin/env bash
# Atualiza registros A na Hostinger quando o IP público muda.
set -euo pipefail

ENV_FILE=/opt/sheeshcord/.env
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
