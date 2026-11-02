# Design

Identidade escolhida pelo usuário: utilitário do sistema, neutro e compacto. Valores ficam em
`src/lib/styles/tokens.css`; este arquivo diz como usá-los. Só o usuário muda a identidade.

## Princípios

- **Consulta rápida:** o painel é estreito e abre por instantes. Conteúdo primeiro, e o mínimo de controles.
- **Densidade:** mais conteúdo por tela, sem apertar a leitura.
- **Discreto:** cor é sinal, não decoração.

## Cor

- Neutros para quase tudo. O destaque azul marca só seleção, estado ligado, link, foco e ação principal.
- Tema claro e escuro seguem o Windows. Toda cor nova nasce nos dois temas, em `light-dark`.
- Vermelho só para erro. Cor fixa fora de `tokens.css` reprova no `check:fast`.

## Tipografia

- Segoe UI em tudo, e Cascadia Mono só em código.
- Corpo em `--font-size-md`. Listas, campos e tabelas em `--font-size-sm`.
- Pesos 400 e 600, sem itálico decorativo.

## Espaço e forma

- Espaços só da escala `--space-*`, de 4 px em 4 px.
- Controles com `--size-control`, barras com `--size-bar` e cantos com `--radius-sm`.
- Linhas de `--color-line` separam áreas; sem sombras.

## Componentes

Use `src/lib/ui/` antes de criar algo. Criar componente só se nada existente servir.

| Componente   | Uso                                                |
| ------------ | -------------------------------------------------- |
| `Button`     | Ação principal, no máximo uma por área             |
| `IconButton` | Ação secundária, sempre com rótulo; liga e desliga |
| `TextField`  | Entrada de uma linha; Esc sai do campo             |
| `Icon`       | Traçado do Lucide, cor do texto, largura 2         |

## Estados

- **Hover:** superfície mais clara, sem deslocar nada.
- **Foco:** contorno de 2 px do destaque, só no teclado.
- **Selecionado ou ligado:** fundo `--color-selected` com texto em destaque.
- **Vazio:** uma frase do que fazer e a ação logo abaixo.
- **Erro:** em vermelho, perto da causa, dizendo o que houve.

## Movimento

- Deslize do painel em 150 ms, definido em `dock.rs`. Hover em `--duration`.
- Nada anima por enfeite nem atrasa a consulta.

## Texto

- Português do Brasil, frases curtas e diretas.
- Botões com verbo no infinitivo: "Abrir", "Escolher pasta".
- Sem exclamação, sem jargão técnico e sem culpar o usuário.

## Conteúdo renderizado

A nota usa só estes tokens, em seletores `:global` dentro de `.note`. A CSP proíbe estilo inline: nenhum
componente depende de atributo `style`.
