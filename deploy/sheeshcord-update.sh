#!/usr/bin/env bash
# Baixa a imagem mais recente do token-server (GHCR) e recria o container só se ela mudou.
# Instalado como timer de usuário (veja deploy/README.md). Se o pull falhar (sem rede, pacote
# ainda não publicado), mantém a imagem atual.
set -euo pipefail

DIR=${SHEESHCORD_DIR:-/opt/sheeshcord}
IMAGE=${TOKEN_SERVER_IMAGE:-ghcr.io/heitormancini/sheeshcord-token-server:latest}

cd "$DIR"
before=$(docker image inspect -f '{{.Id}}' "$IMAGE" 2>/dev/null || true)
docker pull -q "$IMAGE" >/dev/null || { echo "pull de $IMAGE falhou; mantendo a imagem atual" >&2; exit 0; }
after=$(docker image inspect -f '{{.Id}}' "$IMAGE")

running=$(docker inspect -f '{{.Image}}' sheeshcord-token-server-1 2>/dev/null || true)
if [[ $before == "$after" && $running == "$after" ]]; then
  exit 0
fi

TOKEN_SERVER_IMAGE=$IMAGE docker compose up -d --no-deps token-server
docker image prune -f >/dev/null
echo "$(date -Is) token-server atualizado para $after"
