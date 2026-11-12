# Servidor do Sheeshcord na Oracle Cloud (Always Free)

Stack: **LiveKit** (SFU de mídia) + **token-server** (gera tokens, exige a senha do grupo) +
**Caddy** (HTTPS automático) em Docker Compose, numa VM ARM gratuita, acessível em
`https://live.microbates.com`.

```
App ──HTTPS/WSS :443──► Caddy ──/token,/health──► token-server 127.0.0.1:7890
                              └─resto (/rtc)───► livekit 127.0.0.1:7880
App ──UDP 50000–50200 / TCP 7881──────────────► livekit (mídia WebRTC)
```

| Arquivo              | Para quê                                                        |
| -------------------- | --------------------------------------------------------------- |
| `docker-compose.yml` | Os 3 serviços (rede `host`)                                     |
| `livekit.yaml`       | Config de produção do LiveKit (chaves vêm do `.env`)            |
| `Caddyfile`          | Roteamento e TLS                                                |
| `.env.example`       | Modelo das variáveis secretas (copie para `.env` na VM)         |
| `bootstrap.sh`       | Prepara a VM: Docker, firewall, `/opt/sheeshcord`               |
| `livekit.dev.yaml`   | Só para desenvolvimento local e teste de carga                  |

## 1. Conta Oracle Cloud

1. Crie a conta em <https://www.oracle.com/cloud/free/>. O cartão é só para verificação.
2. **Home region: Brazil East (São Paulo)**. Ela não pode ser trocada depois, e os recursos
   Always Free só existem na home region.
3. Opcional, mas recomendado: depois de criada, faça o upgrade para **Pay As You Go**.
   Continua grátis dentro dos limites Always Free, e a Oracle deixa de recuperar VMs
   "ociosas" (uso de CPU/rede abaixo de 20% por 7 dias). Crie um *budget alert* de
   R$ 1 para ser avisado se algo sair do gratuito.

## 2. Criar a VM

Menu ☰ → **Compute → Instances → Create instance**:

- **Image**: Canonical Ubuntu 24.04 (não a "Minimal").
- **Shape**: Ampere → `VM.Standard.A1.Flex`, **2 OCPU / 12 GB** (o Always Free permite até
  4 OCPU / 24 GB no total).
- **Networking**: crie uma VCN nova com sub-rede **pública** e marque "Assign a public IPv4 address".
- **SSH keys**: envie sua chave pública. Para criar uma no Windows:
  `ssh-keygen -t ed25519 -f $HOME\.ssh\oracle_sheeshcord`.
- **Boot volume**: o padrão (~47 GB) basta.

Se aparecer **"Out of capacity"**, tente outro *Availability/Fault domain* ou outro horário
(madrugada costuma funcionar). Com conta Pay As You Go a fila é bem menor.

### IP fixo

**Networking → Reserved public IPs → Reserve**. Depois, na VM: *Attached VNICs → IPv4 addresses*
→ edite o IP primário → *No public IP*, salve; edite de novo → *Reserved public IP* → escolha
o reservado. Assim o IP não muda se a VM for recriada.

### Liberar portas na VCN

**Networking → Virtual cloud networks → (sua VCN) → Security Lists → Default** → *Add Ingress Rules*,
origem `0.0.0.0/0`:

| Protocolo | Porta(s)      | Uso                                      |
| --------- | ------------- | ---------------------------------------- |
| TCP       | 80            | Let's Encrypt (emissão do certificado)   |
| TCP       | 443           | HTTPS/WSS (token + sinalização)          |
| UDP       | 443           | HTTP/3 (opcional)                        |
| TCP       | 7881          | WebRTC por TCP (redes que bloqueiam UDP) |
| UDP       | 50000-50200   | Mídia WebRTC (áudio e vídeo)             |

> A VM também tem firewall próprio (iptables com REJECT nas imagens da Oracle). O
> `bootstrap.sh` cuida dele. **Esquecer qualquer um dos dois = "Aguardando imagem"/sem áudio.**

## 3. DNS (Hostinger)

hPanel → **Domínios → microbates.com → DNS / Nameservers** → adicione:

| Tipo | Nome   | Aponta para          | TTL |
| ---- | ------ | -------------------- | --- |
| A    | `live` | `<IP reservado>`     | 300 |

Confira com `nslookup live.microbates.com` (pode levar alguns minutos).

## 4. Preparar a VM

```bash
ssh -i ~/.ssh/oracle_sheeshcord ubuntu@<IP>
curl -fsSL https://raw.githubusercontent.com/HeitorMancini/microbates-call-app/main/deploy/bootstrap.sh | bash
exit   # entre de novo para o grupo docker valer
```

Copie os arquivos (do seu PC, na raiz do repositório). O deploy automático também faz isso:

```powershell
scp -i $HOME\.ssh\oracle_sheeshcord deploy\docker-compose.yml deploy\Caddyfile deploy\livekit.yaml deploy\.env.example ubuntu@<IP>:/opt/sheeshcord/
```

Crie o `.env` na VM:

```bash
cd /opt/sheeshcord
cp .env.example .env
sed -i "s|^LIVEKIT_API_KEY=.*|LIVEKIT_API_KEY=API$(openssl rand -hex 8)|" .env
sed -i "s|^LIVEKIT_API_SECRET=.*|LIVEKIT_API_SECRET=$(openssl rand -hex 32)|" .env
nano .env    # defina ACCESS_PASSWORD (a senha que o grupo vai digitar no app)
chmod 600 .env
```

Suba tudo:

```bash
docker compose up -d
docker compose ps
docker compose logs -f caddy      # aguarde "certificate obtained successfully"
curl https://live.microbates.com/health   # {"status":"ok"}
```

> A imagem do token-server (`ghcr.io/heitormancini/sheeshcord-token-server`) é gerada pelo
> workflow **Deploy**. Rode-o uma vez (Actions → Deploy → *Run workflow*) antes do primeiro
> `docker compose up`, ou deixe o pacote público (GitHub → Packages → sheeshcord-token-server →
> *Package settings* → *Change visibility → Public*) para a VM baixar sem login.

## 5. Deploy automático (GitHub Actions)

O workflow `.github/workflows/deploy.yml` roda a cada push em `main` que mexa no
token-server ou em `deploy/` (ou manualmente):

1. compila a imagem **ARM64** do token-server num runner ARM e publica no GHCR;
2. copia os arquivos de `deploy/` para `/opt/sheeshcord` via SSH;
3. roda `docker compose pull && docker compose up -d` e checa `https://live.microbates.com/health`.

Configure em **Settings → Secrets and variables → Actions**:

| Tipo     | Nome             | Valor                                                       |
| -------- | ---------------- | ----------------------------------------------------------- |
| Variable | `ORACLE_HOST`    | IP reservado (ou `live.microbates.com`)                     |
| Variable | `ORACLE_USER`    | `ubuntu`                                                    |
| Secret   | `ORACLE_SSH_KEY` | conteúdo da chave **privada** (`oracle_sheeshcord`)         |

Sem `ORACLE_HOST`, o workflow só publica a imagem e pula o deploy. O `.env` com os segredos
**nunca** sai da VM.

## 6. Usar no app

Servidor `https://live.microbates.com` (padrão do app a partir da 0.3.0) + a senha do grupo.
Para trocar a senha: edite `ACCESS_PASSWORD` no `.env` e rode `docker compose up -d`.

## Operação

```bash
cd /opt/sheeshcord
docker compose ps                    # status
docker compose logs -f livekit       # logs (também: token-server, caddy)
docker compose pull && docker compose up -d   # atualizar manualmente
docker compose restart livekit
```

## Problemas comuns

| Sintoma                                     | Verifique                                                                 |
| ------------------------------------------- | ------------------------------------------------------------------------- |
| Caddy não emite o certificado               | DNS aponta para o IP certo? TCP 80/443 abertas na Security List e no iptables? |
| "senha do grupo incorreta"                  | `ACCESS_PASSWORD` no `.env` (sem espaços/aspas extras)                    |
| "muitas tentativas…"                        | 10 senhas erradas em 15 min pelo mesmo IP; aguarde ou `docker compose restart token-server` |
| Entra na sala mas sem áudio/"Aguardando imagem" | UDP 50000-50200 (Security List **e** `sudo iptables -L INPUT -n`); TCP 7881 como fallback |
| Funciona em casa, não no trabalho/faculdade | Rede bloqueando UDP: depende do TCP 7881 (sem TURN nesta configuração)    |
| VM sumiu/parou                              | Recuperação de VM ociosa: converta para Pay As You Go                     |

Para ver o IP que o LiveKit anuncia: `docker compose logs livekit | grep -i "nodeIP"`. Ele deve ser
o IP público (descoberto via STUN por `use_external_ip: true`).
