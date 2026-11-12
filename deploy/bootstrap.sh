#!/usr/bin/env bash
# Prepara uma VM Ubuntu (Oracle Cloud) para o Sheeshcord. Rode uma vez, como usuário com sudo:
#   curl -fsSL https://raw.githubusercontent.com/HeitorMancini/microbates-call-app/main/deploy/bootstrap.sh | bash
set -euo pipefail

APP_DIR=/opt/sheeshcord

echo "==> Instalando Docker"
if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sudo sh
fi
sudo usermod -aG docker "$USER"

echo "==> Liberando portas no firewall da VM (as imagens da Oracle vêm com REJECT no iptables)"
sudo apt-get install -y iptables-persistent >/dev/null
open_port() {
  local proto=$1 port=$2
  if ! sudo iptables -C INPUT -p "$proto" --dport "$port" -m state --state NEW -j ACCEPT 2>/dev/null; then
    sudo iptables -I INPUT 1 -p "$proto" --dport "$port" -m state --state NEW -j ACCEPT
  fi
}
open_port tcp 80          # Let's Encrypt (HTTP-01) e redirecionamento
open_port tcp 443         # HTTPS: token-server + sinalização LiveKit (wss)
open_port udp 443         # HTTP/3 (opcional)
open_port tcp 7881        # LiveKit ICE/TCP (quando UDP é bloqueado)
open_port udp 50000:50200 # LiveKit mídia (WebRTC)
sudo netfilter-persistent save

echo "==> Preparando $APP_DIR"
sudo mkdir -p "$APP_DIR"
sudo chown "$USER":"$USER" "$APP_DIR"

cat <<EOF

Pronto. Próximos passos:
  1. Saia e entre de novo no SSH (para o grupo docker valer).
  2. Copie deploy/ do repositório para $APP_DIR (o workflow de deploy faz isso).
  3. cp $APP_DIR/.env.example $APP_DIR/.env && nano $APP_DIR/.env
  4. cd $APP_DIR && docker compose up -d
EOF
