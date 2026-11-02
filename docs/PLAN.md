# Plano

Cada tarefa indica o nível de modelo (ver `AGENTS.md`). Marque `[x]` no commit que a conclui e detalhe
cada etapa quando ela for a próxima.

## Em andamento

Nada.

## Concluídas

- M0 — Painel provado e aprovado com o Tauri; ver `docs/decisions/0003-painel-e-paginas-web.md`.
- M1 — Consulta de notas em markdown, bandeja, atalho global, CSP e identidade visual; ver `docs/DESIGN.md`.

## Etapa atual: M2 — Criar e editar

- [ ] (padrão) Editor CodeMirror 6 com pré-visualização
- [ ] (avançado) Gravação atômica e autosave
- [ ] (avançado) Reação a alterações externas, sem perda de conteúdo
- [ ] (padrão) Colar imagens como anexos

## M3 — Encontrar

- [ ] (padrão) Índice SQLite FTS5 reconstruível
- [ ] (padrão) Busca rápida por teclado

## M4 — Páginas web

- [ ] (padrão) Salvar páginas como referências, usando a solução escolhida na M0
- [ ] (avançado) Testar se iframes da página disparam esquemas externos sem passar por `on_navigation`

## M5 — Distribuir

- [ ] (padrão) Instalador, atualização automática e definição sobre assinatura de código
- [ ] (avançado) Pôr entre aspas o caminho do autostart: `auto-launch` grava o `Run` sem aspas

## Depois

Sem ordem definida:

- Captura de tela.
- Ditado com modelos locais, baixados à parte do instalador.
- Plugins: exigem uma decisão registrada antes de começar.
- macOS e Linux: nova implementação de `platform/`. No Wayland, o painel depende de layer-shell, que o
  GNOME não oferece.

## Questões em aberto

Valores padrão usados até você decidir o contrário.

| Questão                         | Padrão                                                                            |
| ------------------------------- | --------------------------------------------------------------------------------- |
| Lado da tela                    | Direito, ainda fixo                                                               |
| Monitor                         | O mais à direita, cuja borda segura o cursor                                      |
| Fechar ao tirar o mouse         | Sim, exceto se fixado, com edição ativa ou com a página em foco                   |
| Página sobre apps em tela cheia | Funciona em janela e tela cheia sem bordas; jogos em modo exclusivo ficam de fora |
