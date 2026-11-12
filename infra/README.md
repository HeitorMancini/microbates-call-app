# Infra do Sheeshcord (servidor de voz via Radmin VPN)

Um amigo (o "host") roda o servidor; todos entram na mesma rede Radmin VPN.

## O que sobe

| Serviço        | Porta                         | Função                                   |
| -------------- | ----------------------------- | ---------------------------------------- |
| livekit-server | TCP 7880 (sinalização)        | SFU de áudio/tela                        |
| livekit-server | TCP 7882, UDP 50000–50100     | mídia WebRTC (UDP preferido, TCP reserva) |
| token-server   | TCP 7881                      | `POST /token` gera o JWT de acesso       |

## Uso (no PC do host)

1. Instale o `livekit-server` (v1.13+) em `%USERPROFILE%\.local\bin` ou no `PATH`.
2. Conecte-se ao Radmin VPN e anote seu IP (`26.x.x.x`).
3. Na primeira vez, num PowerShell **como administrador**, libere o firewall:
   ```powershell
   .\infra\start-server.ps1 -OpenFirewall
   ```
4. Nas próximas vezes:
   ```powershell
   .\infra\start-server.ps1               # detecta o IP 26.x.x.x sozinho
   .\infra\start-server.ps1 -NodeIp 26.1.2.3
   ```
5. Os amigos usam no app o servidor `http://<IP-do-host>:7881`.
6. Para parar: `.\infra\stop-server.ps1`.

Logs e PIDs ficam em `infra\.run\`. O token-server é compilado em release
na primeira execução (`-Rebuild` força recompilar).

## Observações

- As chaves em `livekit.yaml` (`devkey` / `devsecret...`) são **só para desenvolvimento**.
  Para trocar, passe `-ApiKey`/`-ApiSecret` (o script repassa ao LiveKit e ao token-server).
- `use_external_ip: false`: o LiveKit anuncia só o IP informado em `-NodeIp`, ideal para VPN.
- Se alguém não conectar áudio, confira se as portas UDP 50000–50100 estão liberadas no host.
