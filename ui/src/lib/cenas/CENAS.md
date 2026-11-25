# Cenas do fundo

O fundo da tela de entrada é uma paisagem ASCII animada. As cenas ficam em
`cenas.json`, nesta pasta. Para criar ou mudar uma cena, edite só esse arquivo.
Não é preciso mexer em código.

Cada vez que o app abre (e cada vez que você sai de uma chamada), ele sorteia uma
cena diferente da anterior. Em seguida, sorteia os valores de cada camada.

## Regras gerais

- **Frações da janela.** Posições e alturas vão de `0` (topo ou esquerda) a `1`
  (base ou direita). Por exemplo, `"base": 0.7` fica a 70% da altura.
- **Faixas.** `[min, max]` sorteia um valor nesse intervalo a cada abertura,
  como em `"x": [0.2, 0.8]`. Um número sozinho vale sempre o mesmo.
- **Horizonte.** Se a cena tem `"horizonte"`, as camadas podem usar
  `"horizonte"`, `"horizonte-0.1"` ou `"horizonte+0.05"`.
- **Cores.** Use um hex (`"#ff3dc8"`) ou uma lista de hex, que vira um degradê
  de cima para baixo (`["#3e4458", "#22252f"]`).
- **`variacaoMatiz`.** Gira o matiz de todas as cores da cena, em graus, a cada
  abertura. Por exemplo, `[-20, 20]`.
- **Ordem das camadas.** As camadas são desenhadas na ordem da lista. A
  seguinte cobre a anterior.
- **Caracteres.** Use `"\\"` para escrever uma barra invertida.

## Campos da cena

| Campo | O que é |
|---|---|
| `nome` | Nome único. É ele que impede a mesma cena de sair duas vezes seguidas. |
| `descricao` | Livre, só para quem edita. |
| `horizonte` | Altura do horizonte (opcional). Padrão: 0,55. |
| `variacaoMatiz` | Giro de matiz por abertura (opcional). |
| `camadas` | Lista de camadas (abaixo). |

## Tipos de camada

Todos os campos são opcionais, exceto `tipo` e `cor` (ou `linhas` e `verticais`
na `grade`).

### `ceu`: faixas de céu em degradê
`ate` (até que altura; padrão 1), `caracteres` (do mais ralo ao mais denso;
padrão `".:-="`), `cor`, `velocidade`.

### `estrelas`: pontos que piscam
`quantidade` (padrão 50), `ate` (altura máxima; padrão 0,5), `caracteres` (o
1º para apagada e o último para acesa; padrão `".*"`), `cor`, `piscar`
(velocidade do pisca; padrão 0,08).

### `astro`: sol ou lua
`forma` (`"disco"` ou `"listrado"`, o sol synthwave), `x`, `y` (centro),
`raio` (em linhas), `caracteres` (miolo e borda; padrão `"@o"`), `cor`,
`limite` (corta o astro abaixo dessa altura; ótimo com `"horizonte"`),
`velocidade` (das listras).

### `cordilheira`: montanhas
`base` (altura média), `relevo` (amplitude dos picos), `escala` (menor gera
montanhas mais largas; padrão 0,08), `velocidade` (deslize lateral; 0 deixa
parado), `crista` (caracteres do pico), `corpo` (caracteres do preenchimento),
`cor` (degradê do pico para baixo), `ate` (para de preencher nessa altura).

### `nevoa`: uma linha de névoa correndo
`y`, `caractere` (padrão `"~"`), `cor`, `densidade` (de 0 a 1; maior deixa mais
rala), `velocidade`.

### `agua`: mar com ondas
`de` (onde começa; normalmente `"horizonte"`), `caracteres` (da crista ao vale;
padrão `"~-._"`), `cor`, `velocidade`, `reflexo` (cor do reflexo do primeiro
`astro`).

### `capim`: colina de capim ao vento
`base`, `relevo`, `escala`, `velocidade` (do vento), `caracteres` (direita,
esquerda, parado e ponta; padrão `"/\\|,"`), `cor`.

### `nuvens`
`ate`, `caracteres` (da mais rala à mais densa; padrão `"-=%"`), `cor`,
`densidade` (maior deixa menos nuvem), `velocidade`.

### `particulas`: vaga-lumes, brasas, neve
`quantidade`, `de`, `ate` (faixa de altura), `caracteres` (fraca e forte;
padrão `"+*"`), `cor`, `piscar`, `deriva` (quanto cada uma passeia).

### `grade`: chão synthwave em perspectiva
`de` (normalmente `"horizonte"`), `linhas` (cor das linhas horizontais),
`verticais` (cor das linhas que fogem), `velocidade`.

## Exemplo: cena nova

```json
{
  "nome": "Neve na floresta",
  "descricao": "Colinas frias com neve caindo.",
  "variacaoMatiz": [-10, 10],
  "camadas": [
    { "tipo": "estrelas", "quantidade": 30, "ate": 0.4, "cor": "#cfe0ff" },
    { "tipo": "cordilheira", "base": 0.6, "relevo": 0.2, "crista": "^", "corpo": "*:.", "cor": ["#e8f0ff", "#5a6a88"] },
    { "tipo": "particulas", "quantidade": 40, "de": 0, "ate": 1, "caracteres": ".*", "cor": "#ffffff", "deriva": 6 }
  ]
}
```

Se o arquivo tiver um erro (tipo desconhecido, cor inválida etc.), o app mostra
um fundo liso e registra a mensagem no console, que indica a cena e a camada.
