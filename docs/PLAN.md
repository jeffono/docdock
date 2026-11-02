# Plano

Cada tarefa indica o nível de modelo (ver `AGENTS.md`). Marque `[x]` no commit que a conclui e detalhe
cada etapa quando ela for a próxima.

## Em andamento

M1 na branch de integração `tarefa/m1`. Próximas: CSP, markdown, atalho global (abre sem o cursor e mexe no
foco) e identidade visual. Falta o usuário testar a bandeja: "Sair" e "Iniciar com o Windows".

## Concluídas

- M0 — Painel provado e aprovado com o Tauri; ver `docs/decisions/0003-painel-e-paginas-web.md`.

## Etapa atual: M1 — Consultar referências

- [x] (padrão) Escolher a pasta de notas e lembrar da escolha
- [ ] (avançado) Definir uma CSP restritiva antes de renderizar conteúdo do usuário
- [x] (padrão) Listar e abrir notas
- [ ] (padrão) Renderizar markdown com tabelas e imagens locais
- [x] (padrão) Ícone na bandeja, instância única e início com o Windows
- [ ] (avançado) Atalho global que abre e fecha o painel
- [ ] (avançado) Primeira definição visual, usando `tokens.css`
- [ ] (padrão) Criar `docs/DESIGN.md` com densidade, ícones, tom dos textos, estados e movimento
- [x] (básico) Reprovar cor fixa fora de `tokens.css` no `check:fast`

**Pronto quando:** você consultar suas referências reais pelo painel no dia a dia.

## M2 — Criar e editar

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
| Lado da tela                    | Direito, configurável na M1                                                       |
| Monitor                         | O mais à direita, cuja borda segura o cursor                                      |
| Fechar ao tirar o mouse         | Sim, exceto se fixado, com edição ativa ou com a página em foco                   |
| Página sobre apps em tela cheia | Funciona em janela e tela cheia sem bordas; jogos em modo exclusivo ficam de fora |
